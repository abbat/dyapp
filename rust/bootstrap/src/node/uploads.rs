//! Gate payload replication with the same bounded ancestry validator as repair.
use super::{
    Behaviour, MediaJob, OutboundRequestId, PeerId, PendingPut, PutRecord, Replicas, Service,
    SignedRecord, Swarm, PUT_TIMEOUT, SHED_MEDIA,
};
use crate::media::MediaStore;
use crate::media_repair::ancestry::History;
use dyapp_p2p_net::proto;
use prost::Message;
use std::collections::HashMap;
use std::time::Instant;

#[cfg(test)]
mod tests;

pub(super) const REPLY_BYTES: u64 = 1024;

enum Admission {
    Legacy,
    Failed,
    Conditional(History),
}

enum Gate {
    Send,
    Wait,
    Drop,
    Request(proto::MediaRequest),
}

struct Flight {
    id: u64,
    peer: PeerId,
    job: MediaJob,
}

#[derive(Default)]
pub(super) struct Uploads {
    states: HashMap<u64, Admission>,
    requests: HashMap<OutboundRequestId, Flight>,
}

fn target(record: &PutRecord) -> Option<&SignedRecord> {
    let (PutRecord::Media(replica) | PutRecord::Sharded(replica, _)) = record else {
        return None;
    };
    match replica.authorization.as_ref()? {
        proto::media_replica_put::Authorization::Keep(record) => Some(record),
        proto::media_replica_put::Authorization::Attach(_) => None,
    }
}

fn put_id(job: MediaJob) -> Option<u64> {
    match job {
        MediaJob::Put(id) | MediaJob::ShardPut(id, _) => Some(id),
        _ => None,
    }
}

fn initial(target: &SignedRecord) -> Admission {
    match History::from_record(target) {
        Ok(Some(history)) => Admission::Conditional(history),
        Ok(None) => Admission::Legacy,
        Err(_) => Admission::Failed,
    }
}

fn next(history: &mut History, peer: PeerId, target: &SignedRecord, media: &MediaStore) -> Gate {
    history.track(peer);
    if history.finished(&peer) {
        return if history.admitted(&peer) {
            Gate::Send
        } else {
            Gate::Drop
        };
    }
    if history.waiting(&peer) {
        return Gate::Wait;
    }
    match history.request(peer, target, media) {
        Ok(Some(request)) => Gate::Request(request),
        Ok(None) if history.admitted(&peer) => Gate::Send,
        _ => {
            history.fail(peer);
            Gate::Drop
        }
    }
}

impl Uploads {
    fn gate(&mut self, id: u64, peer: PeerId, target: &SignedRecord, media: &MediaStore) -> Gate {
        match self.states.entry(id).or_insert_with(|| initial(target)) {
            Admission::Legacy => Gate::Send,
            Admission::Failed => Gate::Drop,
            Admission::Conditional(history) => next(history, peer, target, media),
        }
    }

    fn fail(&mut self, id: u64, peer: PeerId) {
        if let Some(Admission::Conditional(history)) = self.states.get_mut(&id) {
            history.fail(peer);
        }
    }

    fn receive(&mut self, id: u64, peer: PeerId, response: Option<proto::MediaResponse>) -> bool {
        let Some(Admission::Conditional(history)) = self.states.get_mut(&id) else {
            return false;
        };
        if history
            .receive(peer, response)
            .is_ok_and(|progress| progress)
        {
            true
        } else {
            history.fail(peer);
            false
        }
    }

    pub(super) fn has_request(&self, id: OutboundRequestId) -> bool {
        self.requests.contains_key(&id)
    }

    pub(super) fn len(&self) -> usize {
        self.requests.len()
    }

    pub(super) fn active(&self) -> impl Iterator<Item = u64> + '_ {
        self.requests.values().map(|flight| flight.id)
    }

    pub(super) fn prune(&mut self, puts: &HashMap<u64, PendingPut>) {
        self.states.retain(|id, _| puts.contains_key(id));
        // Transport flights remain bounded by media_outgoing until their terminal callbacks.
    }
}

impl Replicas {
    pub(super) fn admit_upload(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        job: MediaJob,
        now: Instant,
    ) -> bool {
        let Some(id) = put_id(job) else {
            return true;
        };
        let Some(put) = self.puts.get(&id) else {
            return false;
        };
        if now >= put.deadline {
            return false;
        }
        let Some(target) = target(&put.record) else {
            return true;
        };
        let Some(media) = service.media.as_ref() else {
            return false;
        };
        match self.uploads.gate(id, to, target, media) {
            Gate::Send => true,
            Gate::Wait => {
                self.media_queue.push_back((to, job));
                false
            }
            Gate::Drop => false,
            Gate::Request(request) => {
                self.send_admission(swarm, service, id, to, job, request);
                false
            }
        }
    }

    fn send_admission(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: u64,
        to: PeerId,
        job: MediaJob,
        request: proto::MediaRequest,
    ) {
        let refused = request
            .request
            .as_ref()
            .is_none_or(|request| super::replicas::listed_media_request(service, request));
        if refused
            || !service
                .traffic
                .try_add(request.encoded_len() as u64 + REPLY_BYTES, SHED_MEDIA)
            || !self.cap.allow(&to, 1)
        {
            self.uploads.fail(id, to);
            return;
        }
        let request_id = swarm.behaviour_mut().media.send_request(&to, request);
        self.media_outgoing.insert(request_id, to);
        self.uploads
            .requests
            .insert(request_id, Flight { id, peer: to, job });
    }

    pub(super) fn upload_reply(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        peer: PeerId,
        request_id: OutboundRequestId,
        response: proto::MediaResponse,
        now: Instant,
    ) {
        service
            .traffic
            .add((response.encoded_len() as u64).saturating_sub(REPLY_BYTES));
        self.upload_response(request_id, Some(response), now);
        self.trust.add(peer, 1);
        self.finish_puts(swarm, service, now);
    }

    pub(super) fn upload_response(
        &mut self,
        request_id: OutboundRequestId,
        response: Option<proto::MediaResponse>,
        now: Instant,
    ) {
        let Some(flight) = self.uploads.requests.remove(&request_id) else {
            return;
        };
        let Some(put) = self.puts.get_mut(&flight.id) else {
            return;
        };
        if now >= put.deadline {
            self.uploads.fail(flight.id, flight.peer);
            return;
        }
        if self.uploads.receive(flight.id, flight.peer, response) {
            put.deadline = put.deadline.max(now + PUT_TIMEOUT);
            // Only a later payload write acknowledgment may confirm this holder.
            self.media_queue.push_back((flight.peer, flight.job));
        }
    }
}
