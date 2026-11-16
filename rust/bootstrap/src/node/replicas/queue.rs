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

    fn repair_reply(
        &mut self,
        id: u64,
        job: usize,
        response: Option<proto::MediaResponse>,
        now: Instant,
    ) {
        if let Some((repair, _)) = self.media_repairs.get_mut(&id) {
            repair.response_at(job, response, now);
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
            self.repair_reply(id, job, None, service.clock.instant());
            return;
        };
        if !clean_repair(service, &mut request) {
            self.repair_reply(id, job, None, service.clock.instant());
            return;
        }
        if to == *swarm.local_peer_id() {
            self.repair_reply(
                id,
                job,
                local_repair(service, to, &request),
                service.clock.instant(),
            );
            return;
        }
        if !service
            .traffic
            .try_add(request.encoded_len() as u64 + reserve, SHED_MEDIA)
            || !self.allow_repair(to, request.request.as_ref().unwrap())
        {
            self.repair_reply(id, job, None, service.clock.instant());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{MediaRepair, PendingPut, PutRecord, ReplicaCap, SignedRecord};

    fn service() -> Service {
        let dir = format!(
            "{}/../../build/test-replica-queue-{}",
            env!("CARGO_MANIFEST_DIR"),
            uuid::Uuid::new_v4()
        );
        let mut config = crate::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.limits.bytes_per_second = 10_000;
        config.limits.min_free_mb = 0;
        let mut service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config);
        service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
        service
    }

    fn store_repair_media(service: &Service) -> (SignedRecord, Vec<u8>) {
        let data = b"queued repair write".to_vec();
        let hash = dyapp_identity::sha256(&data).to_vec();
        let record = dyapp_identity::Identity::generate().sign(
            dyapp_identity::Domain::MediaKeep,
            proto::MediaKeep {
                version: 1,
                hashes: vec![hash.clone()],
                time: chrono::Utc::now().timestamp().unsigned_abs(),
            }
            .encode_to_vec(),
        );
        assert_eq!(
            service
                .store_media(&proto::MediaReplicaPut {
                    data,
                    authorization: Some(proto::media_replica_put::Authorization::Keep(
                        record.clone(),
                    )),
                    ..Default::default()
                })
                .unwrap()
                .status,
            i32::from(proto::Status::Ok)
        );
        (record, hash)
    }

    fn repair_write(service: &Service, to: PeerId) -> (MediaRepair, usize) {
        let (record, hash) = store_repair_media(service);
        let mut repair = MediaRepair::new(service, record, vec![hash], Instant::now()).unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, Some(to));
        }
        repair.advance(service, PeerId::random());
        for (_, job) in repair.queued() {
            let (request, _) = repair.request(job, service).unwrap();
            let Some(proto::media_request::Request::Have(have)) = request.request else {
                panic!("expected repair inventory");
            };
            repair.response(
                job,
                Some(proto::MediaResponse {
                    status: proto::Status::Ok.into(),
                    inventory: Some(proto::MediaInventory {
                        missing: have.items,
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            );
        }
        repair.advance(service, PeerId::random());
        let writes = repair.queued();
        assert_eq!(writes.len(), 1);
        assert!(matches!(
            repair.request(writes[0].1, service).unwrap().0.request,
            Some(proto::media_request::Request::ReplicaPut(_))
        ));
        (repair, writes[0].1)
    }

    #[test]
    fn local_repair_dispatches_inventory_and_both_write_variants() {
        let service = service();
        for request in [
            proto::media_request::Request::Have(proto::MediaHave::default()),
            proto::media_request::Request::ReplicaPut(proto::MediaReplicaPut::default()),
            proto::media_request::Request::ShardPut(proto::MediaShardPut::default()),
        ] {
            assert!(local_repair(
                &service,
                PeerId::random(),
                &proto::MediaRequest {
                    request: Some(request),
                }
            )
            .is_some());
        }
    }

    #[test]
    fn stale_media_jobs_and_exhausted_repair_caps_are_refused() {
        let mut replicas = Replicas {
            cap: ReplicaCap::new(1),
            ..Default::default()
        };
        for job in [
            MediaJob::Put(1),
            MediaJob::ShardPut(1, 0),
            MediaJob::ShardGet(1, 0),
            MediaJob::Repair(1, 0),
        ] {
            assert!(!replicas.media_job_live(&job));
        }
        let peer = PeerId::random();
        let request = proto::media_request::Request::ReplicaPut(proto::MediaReplicaPut::default());
        assert!(replicas.allow_repair(peer, &request));
        assert!(!replicas.allow_repair(peer, &request));
    }

    #[test]
    fn repair_traffic_and_cap_refusals_are_independent() {
        for exhaust_cap in [false, true] {
            let service = service();
            let to = PeerId::random();
            let (repair, job) = repair_write(&service, to);
            let mut replicas = Replicas {
                cap: ReplicaCap::new(1),
                ..Default::default()
            };
            replicas.media_repairs.insert(1, (repair, None));
            if exhaust_cap {
                assert!(replicas.cap.allow(&to, 1));
            } else {
                service.traffic.add(10_000);
            }
            let mut swarm = dyapp_p2p_net::build_swarm(
                libp2p::identity::Keypair::generate_ed25519(),
                dyapp_p2p_net::Mode::Auto,
            )
            .unwrap();
            replicas.drain_repair(&mut swarm, &service, to, 1, job);
            assert!(replicas.media_outgoing.is_empty());
            assert!(replicas.media_repair_requests.is_empty());
            assert!(!replicas.media_repairs[&1].0.has_job(job));
        }
    }

    #[test]
    fn whole_media_put_tracks_reply_and_excludes_prepaid_payload_bytes() {
        let service = service();
        let mut swarm = dyapp_p2p_net::build_swarm(
            libp2p::identity::Keypair::generate_ed25519(),
            dyapp_p2p_net::Mode::Auto,
        )
        .unwrap();
        let replica = proto::MediaReplicaPut {
            data: vec![3; 64],
            ..Default::default()
        };
        let request = proto::MediaRequest {
            request: Some(proto::media_request::Request::ReplicaPut(replica.clone())),
        };
        let expected = request.encoded_len() - replica.data.len();
        let mut replicas = Replicas::default();
        replicas.puts.insert(
            7,
            PendingPut::new(PutRecord::Media(replica), None, false, Instant::now()),
        );
        let before = service.traffic.charged_bytes();
        replicas.send_media(
            &mut swarm,
            &service,
            PeerId::random(),
            MediaJob::Put(7),
            Instant::now(),
        );
        assert_eq!(
            replicas.media_puts.values().copied().collect::<Vec<_>>(),
            [7]
        );
        assert_eq!(replicas.media_outgoing.len(), 1);
        assert_eq!(service.traffic.charged_bytes() - before, expected as u64);
    }
}
