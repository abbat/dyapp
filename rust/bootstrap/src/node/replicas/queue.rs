//! Dispatch one bounded media job without mixing repair callbacks and ordinary puts.
use super::{
    listed_media_request, proto, Behaviour, MediaJob, Peer, PeerId, Replicas, Service, Swarm,
    SHED_MEDIA,
};
use prost::Message;
use std::time::Instant;

fn clean_repair(service: &Service, request: &mut proto::MediaRequest) -> bool {
    if let Some(proto::media_request::Request::Have(have)) = &mut request.request {
        have.items.retain(|part| !service.listed(&part.hash));
    }
    request.request.as_ref().is_some_and(|request| {
        !matches!(request, proto::media_request::Request::Have(have) if have.items.is_empty())
            && !listed_media_request(service, request)
    })
}

fn local_repair(
    service: &Service,
    peer: PeerId,
    request: &proto::MediaRequest,
) -> Option<proto::MediaResponse> {
    match request.request.as_ref()? {
        proto::media_request::Request::KeepCatchup(_) => service
            .media(
                &Peer {
                    id: peer.to_string(),
                    group: "local".into(),
                    coarse: "local".into(),
                },
                request.clone(),
            )
            .ok(),
        proto::media_request::Request::Have(have) => service.media_inventory(&have.items).ok(),
        proto::media_request::Request::ReplicaPut(replica) => service.store_media(replica).ok(),
        proto::media_request::Request::ShardPut(put) => service.store_shard(put).ok(),
        _ => None,
    }
}

impl Replicas {
    pub(super) fn media_job_live(&self, job: &MediaJob) -> bool {
        match job {
            MediaJob::Put(id) | MediaJob::ShardPut(id, _) => self.puts.contains_key(id),
            MediaJob::ShardGet(id, _) => self.media_reads.contains_key(id),
            MediaJob::Repair(id, job) => self
                .media_repairs
                .get(id)
                .is_some_and(|(repair, _)| repair.has_job(*job)),
        }
    }

    pub(super) fn media_busy(&self, to: PeerId) -> bool {
        self.media_outgoing.len() >= 16
            || self
                .media_outgoing
                .values()
                .filter(|peer| **peer == to)
                .count()
                >= 2
    }

    fn allow_repair(&mut self, peer: PeerId, request: &proto::media_request::Request) -> bool {
        if matches!(request, proto::media_request::Request::KeepCatchup(_)) {
            self.cap.allow(&peer, 1)
        } else {
            self.cap.allow_media(&peer, request)
        }
    }

    fn repair_reply(&mut self, id: u64, job: usize, response: Option<proto::MediaResponse>) {
        if let Some((repair, _)) = self.media_repairs.get_mut(&id) {
            repair.response(job, response);
        }
    }

    pub(super) fn drain_repair(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        id: u64,
        job: usize,
    ) {
        let Some((mut request, reserve)) = self
            .media_repairs
            .get_mut(&id)
            .and_then(|(repair, _)| repair.request(job, service))
        else {
            self.repair_reply(id, job, None);
            return;
        };
        if !clean_repair(service, &mut request) {
            self.repair_reply(id, job, None);
            return;
        }
        if to == *swarm.local_peer_id() {
            self.repair_reply(id, job, local_repair(service, to, &request));
            return;
        }
        if !service
            .traffic
            .try_add(request.encoded_len() as u64 + reserve, SHED_MEDIA)
            || !self.allow_repair(to, request.request.as_ref().unwrap())
        {
            self.repair_reply(id, job, None);
            return;
        }
        let request_id = swarm.behaviour_mut().media.send_request(&to, request);
        self.media_outgoing.insert(request_id, to);
        self.media_repair_requests
            .insert(request_id, (id, job, reserve));
    }

    fn queued_media_request(&self, job: &MediaJob) -> Option<proto::media_request::Request> {
        use proto::media_request::Request;
        match job {
            MediaJob::Repair(_, _) => None,
            MediaJob::Put(id) => {
                let record = &self.puts.get(id)?.record;
                let replica = match record {
                    super::PutRecord::Media(replica) | super::PutRecord::Sharded(replica, _) => {
                        replica.clone()
                    }
                    _ => return None,
                };
                Some(Request::ReplicaPut(replica))
            }
            MediaJob::ShardPut(id, index) => {
                let super::PutRecord::Sharded(replica, shards) = &self.puts.get(id)?.record else {
                    return None;
                };
                Some(Request::ShardPut(proto::MediaShardPut {
                    replica: Some(replica.clone()),
                    index: u32::try_from(*index).ok()?,
                    data: shards.get(*index)?.clone(),
                }))
            }
            MediaJob::ShardGet(id, index) => {
                let read = self.media_reads.get(id)?;
                if read.shards.contains_key(index) {
                    return None;
                }
                Some(Request::ShardGet(proto::MediaShardGet {
                    hash: read.manifest.hash.clone(),
                    index: u32::try_from(*index).ok()?,
                }))
            }
        }
    }

    pub(super) fn send_media(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        job: MediaJob,
        now: Instant,
    ) {
        if !self.admit_upload(swarm, service, to, job, now) {
            return;
        }
        let Some(request) = self.queued_media_request(&job) else {
            return;
        };
        if listed_media_request(service, &request) || !self.cap.allow_media(&to, &request) {
            return;
        }
        let prepaid = match &request {
            proto::media_request::Request::ReplicaPut(replica) => replica.data.len(),
            proto::media_request::Request::ShardPut(put) => put.data.len(),
            _ => 0,
        };
        let request = proto::MediaRequest {
            request: Some(request),
        };
        service
            .traffic
            .add((request.encoded_len() - prepaid) as u64);
        let request_id = swarm.behaviour_mut().media.send_request(&to, request);
        self.media_outgoing.insert(request_id, to);
        match job {
            MediaJob::Repair(_, _) => {}
            MediaJob::Put(id) => {
                self.media_puts.insert(request_id, id);
            }
            MediaJob::ShardPut(id, index) => {
                self.shard_puts.insert(request_id, (id, index));
            }
            MediaJob::ShardGet(id, index) => {
                self.shard_gets.insert(request_id, (id, index));
            }
        }
    }
}
