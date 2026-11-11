//! Project completion replies without nesting response handling inside repair loops.
use super::{
    Behaviour, Instant, Lookup, MediaJob, MediaRepairAnswer, PendingPut, PutRecord, Replicas,
    Service, SignedRecord, Status, Swarm,
};
use std::collections::HashSet;

fn written(put: &PendingPut, profiles: &mut Vec<SignedRecord>, media: &mut Vec<Vec<u8>>) {
    match &put.record {
        PutRecord::Media(replica) => media.push(dyapp_identity::sha256(&replica.data).to_vec()),
        PutRecord::Sharded(replica, _) => {
            media.push(replica.manifest.as_ref().unwrap().hash.clone());
        }
        PutRecord::Profile(record) => {
            profiles.push(put.stale.clone().unwrap_or_else(|| record.clone()));
        }
        _ => {}
    }
}

pub(super) fn put_reply(
    put: &mut PendingPut,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    now: Instant,
    profiles: &mut Vec<SignedRecord>,
    media: &mut Vec<Vec<u8>>,
) {
    let Some(status) = put.result(now) else {
        return;
    };
    let Some(reply) = put.reply.take() else {
        return;
    };
    if matches!(status, Status::Ok | Status::Stale) {
        written(put, profiles, media);
    }
    reply.send(swarm, service, status, put.stale.clone());
}

impl Replicas {
    fn active_puts(&self) -> HashSet<u64> {
        let mut active: HashSet<_> = self
            .lookups
            .values()
            .filter_map(|lookup| match lookup {
                Lookup::Put(id) | Lookup::ShardPut(id, _) => Some(*id),
                _ => None,
            })
            .collect();
        active.extend(self.mailbox_puts.values().copied());
        active.extend(self.profile_puts.values().copied());
        active.extend(self.keypackage_puts.values().copied());
        active.extend(self.media_puts.values().copied());
        active.extend(self.uploads.active());
        active.extend(self.shard_puts.values().map(|(id, _)| *id));
        active.extend(self.media_queue.iter().filter_map(|(_, job)| match job {
            MediaJob::Put(id) | MediaJob::ShardPut(id, _) => Some(*id),
            _ => None,
        }));
        active
    }

    pub(super) fn prune_puts(&mut self, swarm: &mut Swarm<Behaviour>, now: Instant) {
        let active = self.active_puts();
        self.puts
            .retain(|id, put| now < put.deadline && (put.reply.is_some() || active.contains(id)));
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::Put(put) | Lookup::ShardPut(put, _) if !self.puts.contains_key(put))
            {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.uploads.prune(&self.puts);
        self.mailbox_puts.retain(|_, id| self.puts.contains_key(id));
        self.profile_puts.retain(|_, id| self.puts.contains_key(id));
        self.keypackage_puts
            .retain(|_, id| self.puts.contains_key(id));
        self.media_puts.retain(|_, id| self.puts.contains_key(id));
        self.shard_puts
            .retain(|_, (id, _)| self.puts.contains_key(id));
        self.lookups.retain(|_, lookup| !matches!(lookup, Lookup::Put(id) | Lookup::ShardPut(id, _) if !self.puts.contains_key(id)));
    }
}

pub(super) fn missing(answer: &mut Option<MediaRepairAnswer>, hash: &[u8], missing: bool) {
    let Some((_, response)) = answer else {
        return;
    };
    response.missing.retain(|entry| entry.as_slice() != hash);
    if missing {
        response.missing.push(hash.to_vec());
    }
}
