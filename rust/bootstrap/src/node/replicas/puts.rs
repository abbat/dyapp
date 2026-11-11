//! Prepare immutable put records and schedule validated shard positions.
use super::{
    mailbox_request, proto, push, replica_key, Behaviour, Lookup, MailboxRequest, MediaJob, PeerId,
    PendingPut, PutRecord, Replicas, Service, SignedRecord, Status, Swarm, Watchers,
};
use crate::replication::media_encode;
use prost::Message;
use std::sync::Arc;

pub(super) fn encode(record: PutRecord, threshold: usize) -> crate::Result<PutRecord> {
    match record {
        PutRecord::Media(mut replica) if replica.data.len() > threshold => {
            let (manifest, shards) = media_encode(&replica.data)?;
            replica.data.clear();
            replica.manifest = Some(manifest);
            Ok(PutRecord::Sharded(replica, Arc::new(shards)))
        }
        record => Ok(record),
    }
}

pub(super) fn key(record: &PutRecord) -> Result<Vec<u8>, ()> {
    Ok(match record {
        PutRecord::Mailbox(record, _) => {
            proto::Envelope::decode(record.payload.as_slice())
                .unwrap()
                .mailbox
        }
        PutRecord::Profile(record) => {
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec()
        }
        PutRecord::KeyPackage(record) => {
            proto::KeyPackageRecord::decode(record.payload.as_slice())
                .map_err(|_| ())?
                .owner
        }
        PutRecord::Media(replica) => dyapp_identity::sha256(&replica.data).to_vec(),
        PutRecord::Sharded(replica, _) => replica.manifest.as_ref().unwrap().hash.clone(),
    })
}

type Confirmation = (i32, Option<SignedRecord>);

fn local_result(
    record: &PutRecord,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    watchers: &Watchers,
) -> crate::Result<Confirmation> {
    Ok(match record {
        PutRecord::Mailbox(record, group) => {
            let response =
                service.store_mailbox(record, &swarm.local_peer_id().to_string(), group)?;
            if response.status == i32::from(Status::Ok) {
                push(swarm, watchers, vec![record.clone()]);
            }
            (response.status, None)
        }
        PutRecord::Profile(record) => {
            let response = service.store_profile(record)?;
            (response.status, response.record)
        }
        PutRecord::KeyPackage(record) => (service.store_keypackage(record)?.status, None),
        PutRecord::Media(replica) | PutRecord::Sharded(replica, _) => {
            (service.store_media(replica)?.status, None)
        }
    })
}

pub(super) fn local(
    put: &mut PendingPut,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    watchers: &Watchers,
    to: PeerId,
) {
    let kind = match &put.record {
        PutRecord::Mailbox(..) => "mailbox",
        PutRecord::Profile(_) => "profile",
        PutRecord::KeyPackage(_) => "KeyPackage",
        PutRecord::Media(_) | PutRecord::Sharded(..) => "media",
    };
    match local_result(&put.record, swarm, service, watchers) {
        Ok((status, record)) => put.confirm(to, status, record),
        Err(error) => tracing::error!(%error, "local {kind} replica failed"),
    }
}

impl Replicas {
    pub(super) fn remote_put(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: u64,
        to: PeerId,
    ) {
        let Some(put) = self.puts.get(&id) else {
            return;
        };
        let record = &put.record;
        match record {
            PutRecord::Mailbox(record, group) => {
                let request = MailboxRequest {
                    request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                        envelopes: vec![record.clone()],
                        source_groups: vec![group.clone()],
                        remaining_seconds: Vec::new(),
                    })),
                };
                service.traffic.add(request.encoded_len() as u64);
                let request_id = swarm.behaviour_mut().mailbox.send_request(&to, request);
                self.mailbox_puts.insert(request_id, id);
            }
            PutRecord::Profile(record) => {
                let request = proto::ProfileRequest {
                    request: Some(proto::profile_request::Request::ReplicaPut(record.clone())),
                    age_seconds: 0,
                };
                service.traffic.add(request.encoded_len() as u64);
                let request_id = swarm.behaviour_mut().profile.send_request(&to, request);
                self.profile_puts.insert(request_id, id);
            }
            PutRecord::KeyPackage(record) => {
                let request = proto::KeyPackageRequest {
                    request: Some(proto::key_package_request::Request::ReplicaPut(
                        record.clone(),
                    )),
                };
                service.traffic.add(request.encoded_len() as u64);
                let request_id = swarm.behaviour_mut().key_package.send_request(&to, request);
                self.keypackage_puts.insert(request_id, id);
            }
            PutRecord::Media(_) | PutRecord::Sharded(_, _) => {
                self.media_queue.push_back((to, MediaJob::Put(id)));
            }
        }
    }
    pub(super) fn shard_lookups(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: u64,
        key: &[u8],
        count: usize,
        isolated: bool,
    ) {
        for index in 0..count {
            if isolated {
                self.shard_holder(swarm, service, id, index, *swarm.local_peer_id());
            } else if let Ok(position) = u8::try_from(index) {
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(key, position));
                self.lookups.insert(query, Lookup::ShardPut(id, index));
            }
        }
    }
}
