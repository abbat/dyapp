//! Replication, ack forwarding and repair between nodes.

use super::*;
#[path = "replicas/finish.rs"]
mod finish;
#[path = "replicas/inventory.rs"]
mod inventory;
#[path = "replicas/puts.rs"]
mod puts;
#[path = "replicas/queue.rs"]
mod queue;

pub(super) fn listed_media_request(
    service: &Service,
    request: &proto::media_request::Request,
) -> bool {
    use proto::media_request::Request;
    match request {
        Request::Get(get) => service.listed(&get.hash),
        Request::ShardGet(get) => service.listed(&get.hash),
        Request::ReplicaPut(replica) => replica.manifest.as_ref().map_or_else(
            || service.listed(&dyapp_identity::sha256(&replica.data)),
            |manifest| service.listed(&manifest.hash),
        ),
        Request::ShardPut(put) => put.replica.as_ref().is_some_and(|replica| {
            replica
                .manifest
                .as_ref()
                .is_some_and(|manifest| service.listed(&manifest.hash))
        }),
        _ => false,
    }
}

impl Replicas {
    /// Entries that wait for a reply, a lookup or a queue slot; each leaves on its answer, failure
    /// or timeout, so an idle node holds none.
    pub(super) fn pending(&self) -> usize {
        self.puts.len()
            + self.mailbox_puts.len()
            + self.profile_puts.len()
            + self.keypackage_puts.len()
            + self.media_puts.len()
            + self.shard_puts.len()
            + self.shard_gets.len()
            + self.media_reads.len()
            + self.media_repairs.len()
            + self.media_repair_requests.len()
            + self.media_queue.len()
            + self.media_outgoing.len()
            + self.lookups.len()
            + self.inventories.len()
            + self.profile_inventories.len()
            + self.republish.len()
    }

    pub(super) fn repair_media(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        record: SignedRecord,
        reply: libp2p::request_response::ResponseChannel<proto::MediaResponse>,
        mut response: proto::MediaResponse,
        now: Instant,
    ) {
        // A blob written before its removal is missing again if the owner lists it anew.
        if let Some(Ok(removed)) = service
            .media
            .as_ref()
            .map(super::super::media::MediaStore::take_removed)
        {
            for hash in removed {
                self.media_repaired.forget(&hash);
            }
        }
        let keep = proto::MediaKeep::decode(record.payload.as_slice()).unwrap();
        for hash in &keep.hashes {
            if service.listed(hash) {
                continue;
            }
            if let Some(missing) = self.media_repaired.result(hash, now) {
                response.missing.retain(|h| h != hash);
                if missing {
                    response.missing.push(hash.clone());
                }
            }
        }
        if let Some((_, answer)) = self
            .start_media_repair(swarm, service, record, now)
            .and_then(|id| self.media_repairs.get_mut(&id))
        {
            *answer = Some((reply, response));
            self.finish_puts(swarm, service, now);
        } else {
            service.traffic.add(response.encoded_len() as u64);
            let _ = swarm.behaviour_mut().media.send_response(reply, response);
        }
    }

    pub(super) fn start_media_repair(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        record: SignedRecord,
        now: Instant,
    ) -> Option<u64> {
        if service.traffic.second() >= SHED_MEDIA || self.media_repairs.len() >= 4 {
            return None;
        }
        let keep = proto::MediaKeep::decode(record.payload.as_slice()).ok()?;
        let owner =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        let allowed: Vec<_> = keep
            .hashes
            .iter()
            .filter(|h| !service.listed(h))
            .cloned()
            .collect();
        let hashes = self.media_repaired.select(owner, &allowed, now);
        if hashes.is_empty() {
            return None;
        }
        let mut repair = match MediaRepair::new(service, record, hashes, now) {
            Ok(repair) => repair,
            Err(error) => {
                tracing::error!(%error, "media repair initialization failed");
                return None;
            }
        };
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        let isolated = dyapp_p2p_net::known_peers(swarm).is_empty();
        let hashes: Vec<_> = repair.hashes().map(<[u8]>::to_vec).collect();
        for (blob, hash) in hashes.iter().enumerate() {
            for index in repair.keys(blob) {
                if isolated {
                    repair.holder(blob, index, Some(*swarm.local_peer_id()));
                } else {
                    let query = swarm
                        .behaviour_mut()
                        .kad
                        .get_closest_peers(replica_key(hash, index));
                    self.lookups
                        .insert(query, Lookup::MediaRepair(id, blob, index));
                }
            }
        }
        self.media_repairs.insert(id, (repair, None));
        Some(id)
    }

    pub(super) fn media_inventory(
        &self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        peer: PeerId,
        items: &[proto::MediaPart],
    ) -> crate::Result<proto::MediaResponse> {
        let allowed = if self.trust.trusted(&peer) {
            inventory::allowed(swarm, peer, items)
        } else {
            Vec::new()
        };
        let response = service.media_inventory(&allowed)?;
        if service
            .traffic
            .try_add(response.encoded_len() as u64, SHED_MEDIA)
        {
            Ok(response)
        } else {
            Ok(proto::MediaResponse {
                status: Status::RateLimited.into(),
                ..Default::default()
            })
        }
    }

    pub(super) fn finish_media_repairs(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        now: Instant,
    ) -> Vec<(u64, bool)> {
        let mut done = Vec::new();
        for (id, (repair, _)) in &mut self.media_repairs {
            if now < repair.deadline {
                repair.advance(service, *swarm.local_peer_id());
                for (peer, job) in repair.queued() {
                    self.media_queue
                        .push_back((peer, MediaJob::Repair(*id, job)));
                }
            }
            if repair.done() || now >= repair.deadline {
                done.push(*id);
            }
        }
        let mut finished = Vec::with_capacity(done.len());
        for id in done {
            let (repair, mut answer) = self.media_repairs.remove(&id).unwrap();
            let mut complete = true;
            for (hash, missing) in repair.results() {
                let missing = missing || service.listed(hash);
                complete &= !missing;
                self.media_repaired.finish(hash, missing);
                finish::missing(&mut answer, hash, missing);
            }
            if let Some((reply, response)) = answer {
                service.traffic.add(response.encoded_len() as u64);
                let _ = swarm.behaviour_mut().media.send_response(reply, response);
            }
            finished.push((id, complete));
        }
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::MediaRepair(repair, _, _) if !self.media_repairs.contains_key(repair))
            {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.lookups.retain(|_, l| !matches!(l, Lookup::MediaRepair(id, _, _) if !self.media_repairs.contains_key(id)));
        finished
    }

    /// Looks up the closest nodes of `record`'s replica keys, at most once an hour per profile;
    /// returns the lookups.
    pub(super) fn repair_profile(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        record: SignedRecord,
        now: Instant,
    ) -> Vec<kad::QueryId> {
        if service.traffic.second() >= SHED_MEDIA || dyapp_p2p_net::known_peers(swarm).is_empty() {
            return Vec::new();
        }
        let Ok(verified) = dyapp_profile::verify(&record) else {
            return Vec::new();
        };
        let key =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        self.profile_repaired
            .retain(|_, (at, _)| now < *at + PROFILE_REPAIR_INTERVAL);
        if self.profile_repaired.contains_key(&key)
            || self.profile_repaired.len() >= MAX_PROFILE_REPAIRS
        {
            return Vec::new();
        }
        let record = match service.store.get_profile(&verified.peer_id) {
            Ok(Some(held))
                if dyapp_profile::stored_version(&held)
                    .is_ok_and(|version| version > verified.profile.version) =>
            {
                held
            }
            Ok(_) => record,
            Err(error) => {
                tracing::error!(%error, "profile repair failed");
                return Vec::new();
            }
        };
        self.profile_repaired
            .insert(key.clone(), (now, HashSet::new()));
        let record = Arc::new(record);
        (0..REPLICAS)
            .map(|i| {
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&key, i));
                self.lookups
                    .insert(query, Lookup::ProfileRepair(record.clone()));
                query
            })
            .collect()
    }

    /// Asks `to` which version of `record` it holds; returns the request, or `Err` when traffic
    /// refused it.
    pub(super) fn profile_inventory(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        record: Arc<SignedRecord>,
    ) -> Result<Option<OutboundRequestId>, ()> {
        if to == *swarm.local_peer_id() || !self.trust.trusted(&to) {
            return Ok(None);
        }
        let key =
            dyapp_identity::key_hash(&record.public_key.as_slice().try_into().unwrap()).to_vec();
        let Some((_, sent)) = self.profile_repaired.get_mut(&key) else {
            return Ok(None);
        };
        if sent.contains(&to) {
            return Ok(None);
        }
        let request = proto::ProfileRequest {
            request: Some(proto::profile_request::Request::Inventory(
                proto::GetProfile { peer_id: key },
            )),
            age_seconds: 0,
        };
        if !service
            .traffic
            .try_add(request.encoded_len() as u64, SHED_MEDIA)
        {
            return Err(());
        }
        sent.insert(to);
        let id = swarm.behaviour_mut().profile.send_request(&to, request);
        self.profile_inventories.insert(id, record);
        Ok(Some(id))
    }

    pub(super) fn profile_gap(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        to: PeerId,
        record: &SignedRecord,
        response: &proto::ProfileResponse,
    ) -> Result<Option<OutboundRequestId>, ()> {
        let Ok(version) = dyapp_profile::stored_version(record) else {
            return Ok(None);
        };
        if response.status != i32::from(Status::NotFound)
            && !(response.status == i32::from(Status::Ok) && response.version < version)
        {
            return Ok(None);
        }
        let Ok(key) = <[u8; 32]>::try_from(record.public_key.as_slice()) else {
            return Ok(None);
        };
        let last_seen = service
            .store
            .profile_last_seen(&dyapp_identity::peer_id(&key))
            .ok()
            .flatten();
        let now = chrono::Utc::now().timestamp();
        let request = proto::ProfileRequest {
            request: Some(proto::profile_request::Request::ReplicaPut(record.clone())),
            age_seconds: last_seen.map_or(0, |seen| {
                u32::try_from((now - seen).max(0)).unwrap_or(u32::MAX)
            }),
        };
        if !self.trust.trusted(&to) {
            return Ok(None);
        }
        if service
            .traffic
            .try_add(request.encoded_len() as u64, SHED_MEDIA)
            && self.cap.allow(&to, 1)
        {
            Ok(Some(
                swarm.behaviour_mut().profile.send_request(&to, request),
            ))
        } else {
            Err(())
        }
    }

    pub(super) fn at_capacity(&self) -> bool {
        self.puts.len() >= MAX_PENDING_PUTS
    }

    pub(super) fn start_put(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        watchers: &Watchers,
        record: PutRecord,
        reply: PutReply,
    ) {
        if self.at_capacity() {
            reply.send(swarm, service, Status::Full, None);
            return;
        }
        let record = match puts::encode(record, service.config.media.shard_threshold) {
            Ok(record) => record,
            Err(error) => {
                tracing::error!(%error, "media encode failed");
                reply.send(swarm, service, Status::Invalid, None);
                return;
            }
        };
        let shard_count = match &record {
            PutRecord::Sharded(_, shards) => shards.len(),
            _ => 0,
        };
        let isolated = dyapp_p2p_net::known_peers(swarm).is_empty();
        let Ok(key) = puts::key(&record) else {
            reply.send(swarm, service, Status::Invalid, None);
            return;
        };
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        self.puts
            .insert(id, PendingPut::new(record, Some(reply), isolated));
        if isolated {
            self.put_holder(swarm, service, watchers, id, *swarm.local_peer_id());
        } else {
            for index in 0..REPLICAS {
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&key, index));
                self.lookups.insert(query, Lookup::Put(id));
            }
        }
        self.shard_lookups(swarm, service, id, &key, shard_count, isolated);
        self.finish_puts(swarm, service, Instant::now());
    }

    pub(super) fn put_holder(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        watchers: &Watchers,
        id: u64,
        to: PeerId,
    ) {
        let Some(put) = self.puts.get_mut(&id) else {
            return;
        };
        if !put.sent.insert(to) {
            return;
        }
        if to == *swarm.local_peer_id() {
            puts::local(put, swarm, service, watchers, to);
            return;
        }
        if !self.trust.trusted(&to) {
            return;
        }
        if matches!(
            put.record,
            PutRecord::Mailbox(..) | PutRecord::Profile(_) | PutRecord::KeyPackage(_)
        ) && !self.cap.allow(&to, 1)
        {
            return;
        }
        self.remote_put(swarm, service, id, to);
    }

    pub(super) fn finish_puts(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        now: Instant,
    ) {
        let mut profiles = Vec::new();
        let mut media_written = Vec::new();
        for (id, put) in &mut self.puts {
            if matches!(put.record, PutRecord::Media(_) | PutRecord::Sharded(_, _))
                && *id >= self.media_holders.0
            {
                self.media_holders = (*id, put.stored.len());
            }
            finish::put_reply(put, swarm, service, now, &mut profiles, &mut media_written);
        }
        for hash in media_written {
            self.media_repaired.finish(&hash, false);
        }
        for record in profiles {
            self.repair_profile(swarm, service, record, now);
        }
        self.prune_puts(swarm, now);
        self.finish_reads(swarm, service, now);
        for (repair, complete) in self.finish_media_repairs(swarm, service, now) {
            self.republish.media_done(repair, complete);
        }
        self.drain_media(swarm, service);
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "Indices come from the validated media layout, at most six data shards plus four parity shards"
    )]
    pub(super) fn shard_holder(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        id: u64,
        index: usize,
        to: PeerId,
    ) {
        let Some(put) = self.puts.get_mut(&id) else {
            return;
        };
        let PutRecord::Sharded(replica, shards) = &put.record else {
            return;
        };
        if !put.shard_sent.insert((index, to)) {
            return;
        }
        if to == *swarm.local_peer_id() {
            let request = proto::MediaShardPut {
                replica: Some(replica.clone()),
                index: index as u32,
                data: shards[index].clone(),
            };
            match service.store_shard(&request) {
                Ok(response) if response.status == i32::from(Status::Ok) => {
                    put.shard_stored.insert(index);
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "local shard failed"),
            }
        } else if self.trust.trusted(&to) {
            self.media_queue
                .push_back((to, MediaJob::ShardPut(id, index)));
        }
    }

    /// The remote media codec admits two streams per connection: queue excess work locally.
    pub(super) fn drain_media(&mut self, swarm: &mut Swarm<Behaviour>, service: &Service) {
        let count = self.media_queue.len();
        for _ in 0..count {
            let (to, job) = self.media_queue.pop_front().unwrap();
            if !self.media_job_live(&job) {
                continue;
            }
            if self.media_busy(to) {
                self.media_queue.push_back((to, job));
                continue;
            }
            match job {
                MediaJob::Repair(id, job) => self.drain_repair(swarm, service, to, id, job),
                job => self.send_media(swarm, service, to, job),
            }
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "Indices come from the validated media layout, at most six data shards plus four parity shards"
    )]
    pub(super) fn start_read(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        manifest: proto::MediaManifest,
        get: proto::GetMedia,
        reply: libp2p::request_response::ResponseChannel<proto::MediaResponse>,
    ) {
        let id = self.next_put;
        self.next_put = self.next_put.wrapping_add(1);
        let mut read = MediaRead {
            manifest,
            get,
            reply,
            deadline: Instant::now() + PUT_TIMEOUT,
            shards: HashMap::new(),
        };
        for index in 0..read.manifest.shard_hashes.len() {
            match service
                .media
                .as_ref()
                .unwrap()
                .shard(&read.manifest.hash, index)
            {
                Ok(Some(data)) if read.manifest.accepts(index, &data) => {
                    read.shards.insert(index, data);
                }
                Ok(_) => {}
                Err(error) => tracing::error!(%error, "local shard read failed"),
            }
        }
        let hash = read.manifest.hash.clone();
        let count = read.manifest.shard_hashes.len();
        self.media_reads.insert(id, read);
        if self.media_reads[&id].shards.len()
            < self.media_reads[&id].manifest.data_shards().unwrap()
        {
            for index in 0..count {
                if self.media_reads[&id].shards.contains_key(&index) {
                    continue;
                }
                let query = swarm
                    .behaviour_mut()
                    .kad
                    .get_closest_peers(replica_key(&hash, index as u8));
                self.lookups.insert(query, Lookup::ShardGet(id, index));
            }
        }
        self.finish_puts(swarm, service, Instant::now());
    }

    pub(super) fn finish_reads(
        &mut self,
        swarm: &mut Swarm<Behaviour>,
        service: &Service,
        now: Instant,
    ) {
        let done: Vec<_> = self
            .media_reads
            .iter()
            .filter_map(|(id, read)| {
                (now >= read.deadline || read.shards.len() >= read.manifest.data_shards().unwrap())
                    .then_some(*id)
            })
            .collect();
        for id in done {
            let read = self.media_reads.remove(&id).unwrap();
            let shards: Vec<_> = read.shards.into_iter().collect();
            let response = if shards.len() >= read.manifest.data_shards().unwrap() {
                service
                    .assemble_media(&read.manifest, &shards, &read.get)
                    .unwrap_or_else(|error| {
                        tracing::error!(%error, "media assembly failed");
                        proto::MediaResponse {
                            status: Status::NotFound.into(),
                            ..Default::default()
                        }
                    })
            } else {
                proto::MediaResponse {
                    status: Status::NotFound.into(),
                    ..Default::default()
                }
            };
            service
                .traffic
                .add((response.encoded_len() - response.data.len()) as u64);
            let _ = swarm
                .behaviour_mut()
                .media
                .send_response(read.reply, response);
        }
        for (id, lookup) in &self.lookups {
            if matches!(lookup, Lookup::ShardGet(read, _) if !self.media_reads.contains_key(read)) {
                if let Some(mut query) = swarm.behaviour_mut().kad.query_mut(id) {
                    query.finish();
                }
            }
        }
        self.lookups.retain(
            |_, l| !matches!(l, Lookup::ShardGet(id, _) if !self.media_reads.contains_key(id)),
        );
    }
}
