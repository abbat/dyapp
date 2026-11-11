//! Destination acknowledgments govern handover completion, independently of readable sources.
use super::*;

fn remote(swarm: &mut Swarm<Behaviour>) -> PeerId {
    let address = dyapp_p2p_net::known_peers(swarm).pop().unwrap();
    let Some(libp2p::multiaddr::Protocol::P2p(peer)) = address.iter().last() else {
        panic!("missing routing peer");
    };
    peer
}

#[tokio::test]
async fn queued_ancestry_filters_owner_refusal_without_blocking_hash_metadata() {
    use crate::node::replicas::listed_media_request;
    let (source, swarm) = node();
    let owner = dyapp_identity::Identity::from_secret(&[71; 32]);
    let hash = dyapp_identity::sha256(b"blocked bytes").to_vec();
    let record = conditional_record(&swarm, hash.clone());
    let request = proto::media_request::Request::KeepCatchup(proto::MediaKeepCatchup {
        target: Some(record),
        ancestors: vec![],
    });
    source
        .deny
        .write()
        .unwrap()
        .insert(dyapp_identity::hex(&hash));
    assert!(!listed_media_request(&source, &request));
    source.deny.write().unwrap().insert(owner.peer_id());
    assert!(listed_media_request(&source, &request));
}

fn conditional_record(swarm: &Swarm<Behaviour>, hash: Vec<u8>) -> dyapp_identity::SignedRecord {
    dyapp_identity::Identity::from_secret(&[71; 32]).sign(
        dyapp_identity::Domain::MediaConditionalKeep,
        proto::MediaKeepIntent {
            version: 1,
            hashes: vec![hash],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
            requester: swarm.local_peer_id().to_string(),
            expected: Some(proto::MediaKeepCondition::default()),
            intent_hash: vec![1; 32],
            ..Default::default()
        }
        .encode_to_vec(),
    )
}

#[tokio::test]
async fn conditional_history_dispatch_uses_local_validation_and_remote_accounting() {
    for local in [true, false] {
        let (source, mut swarm) = node();
        let hash = dyapp_identity::sha256(b"dispatch photo").to_vec();
        let record = conditional_record(&swarm, hash.clone());
        let to = if local {
            *swarm.local_peer_id()
        } else {
            remote(&mut swarm)
        };
        let mut repair =
            crate::media_repair::Repair::new(&source, record.clone(), vec![hash], Instant::now())
                .unwrap();
        for index in repair.keys(0) {
            repair.holder(0, index, Some(to));
        }
        repair.advance(&source, *swarm.local_peer_id());
        let queued = repair.queued();
        assert_eq!(queued.len(), 1);
        let job = queued[0].1;
        let mut replicas = Replicas::default();
        replicas.media_repairs.insert(1, (repair, None));
        replicas
            .media_queue
            .push_back((to, MediaJob::Repair(1, job)));
        replicas.drain_media(&mut swarm, &source);
        assert_eq!(replicas.media_outgoing.len(), usize::from(!local));
        assert_eq!(replicas.media_repair_requests.len(), usize::from(!local));
        let repair = &replicas.media_repairs.get(&1).unwrap().0;
        assert_eq!(repair.has_job(job), !local);
        let owner = dyapp_identity::Identity::from_secret(&[71; 32]).peer_id();
        let snapshot = source
            .media
            .as_ref()
            .unwrap()
            .keep_snapshot(&owner)
            .unwrap();
        assert_eq!(snapshot.record.as_ref(), local.then_some(&record));
    }
}

struct Transfer {
    source: Service,
    destination: Service,
    swarm: Swarm<Behaviour>,
    replicas: Replicas,
    id: u64,
    now: Instant,
    hash: Vec<u8>,
    remote: PeerId,
}

impl Transfer {
    fn new() -> Self {
        let mut transfer = Self::begin();
        transfer.resolve(Some(transfer.remote));
        transfer
    }

    fn resolve(&mut self, peer: Option<PeerId>) {
        let repair = &mut self.replicas.media_repairs.get_mut(&self.id).unwrap().0;
        for index in repair.keys(0) {
            repair.holder(0, index, peer);
        }
    }

    fn begin() -> Self {
        let (source, mut swarm) = node();
        let (destination, _) = node();
        let hash = dyapp_identity::sha256(b"handover bytes").to_vec();
        let record = dyapp_identity::Identity::from_secret(&[71; 32]).sign(
            dyapp_identity::Domain::MediaKeep,
            proto::MediaKeep {
                version: 1,
                hashes: vec![hash.clone()],
                time: chrono::Utc::now().timestamp().unsigned_abs(),
            }
            .encode_to_vec(),
        );
        assert_eq!(
            source
                .store_media(&proto::MediaReplicaPut {
                    data: b"handover bytes".to_vec(),
                    authorization: Some(proto::media_replica_put::Authorization::Keep(record)),
                    ..Default::default()
                })
                .unwrap()
                .status,
            i32::from(Status::Ok)
        );
        let mut replicas = Replicas::default();
        let now = ready(&mut replicas);
        replicas.scan(&mut swarm, &source, now);
        replicas.republish(&mut swarm, &source, now);
        assert_eq!(replicas.media_repairs.len(), 1);
        let id = *replicas.media_repairs.keys().next().unwrap();
        let remote = remote(&mut swarm);

        Self {
            source,
            destination,
            swarm,
            replicas,
            id,
            now,
            hash,
            remote,
        }
    }

    fn request(&mut self) -> (usize, proto::media_request::Request) {
        assert_eq!(
            self.replicas
                .finish_media_repairs(&mut self.swarm, &self.source, self.now),
            []
        );
        let queued = self.replicas.media_queue.pop_front().unwrap();
        assert_eq!(queued.0, self.remote);
        assert_eq!(self.replicas.media_queue.len(), 0);
        let MediaJob::Repair(id, job) = queued.1 else {
            panic!("expected repair job");
        };
        assert_eq!(id, self.id);
        let request = self
            .replicas
            .media_repairs
            .get_mut(&id)
            .unwrap()
            .0
            .request(job, &self.source)
            .unwrap()
            .0
            .request
            .unwrap();
        (job, request)
    }

    fn answer(&mut self, job: usize, response: Option<proto::MediaResponse>) {
        self.replicas
            .media_repairs
            .get_mut(&self.id)
            .unwrap()
            .0
            .response(job, response);
    }

    fn inventory(&mut self) {
        let (job, proto::media_request::Request::Have(have)) = self.request() else {
            panic!("expected inventory");
        };
        let response = self.destination.media_inventory(&have.items).unwrap();
        self.answer(job, Some(response));
    }

    fn write(&mut self) -> (usize, proto::MediaReplicaPut) {
        let (job, proto::media_request::Request::ReplicaPut(put)) = self.request() else {
            panic!("expected write");
        };
        (job, put)
    }

    fn settle(&mut self) -> bool {
        let finished = self
            .replicas
            .finish_media_repairs(&mut self.swarm, &self.source, self.now);
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].0, self.id);
        self.replicas.republish.media_done(self.id, finished[0].1);
        self.replicas
            .republish(&mut self.swarm, &self.source, self.now);
        assert_eq!(self.replicas.republish.len(), 0);
        self.replicas.scan(&mut self.swarm, &self.source, self.now);
        self.replicas.republish.scanner.pop(Store::Media).is_some()
    }

    fn available(&self) {
        assert_eq!(
            self.source.media.as_ref().unwrap().get(&self.hash).unwrap(),
            Some(b"handover bytes".to_vec())
        );
        assert_eq!(
            self.replicas.media_repaired.result(&self.hash, self.now),
            Some(false)
        );
    }
}

#[tokio::test]
async fn refused_media_write_keeps_handover_dirty_without_reupload() {
    let mut replies = vec![None];
    replies.extend(
        [
            Status::Denied,
            Status::Full,
            Status::NotFound,
            Status::Stale,
            Status::Unsupported,
            Status::Invalid,
        ]
        .map(|status| {
            Some(proto::MediaResponse {
                status: status.into(),
                ..Default::default()
            })
        }),
    );
    for response in replies {
        let mut transfer = Transfer::new();
        transfer.inventory();
        let (job, _) = transfer.write();
        let rejected = response.clone();
        transfer.answer(job, response);
        assert!(
            transfer.settle(),
            "refused destination was committed for {rejected:?}"
        );
        transfer.available();
    }
}

#[tokio::test]
async fn wrong_operation_receipts_do_not_confirm_a_write() {
    let ok = proto::MediaResponse {
        status: Status::Ok.into(),
        ..Default::default()
    };
    let replies = [
        proto::MediaResponse {
            data: vec![1],
            ..ok.clone()
        },
        proto::MediaResponse {
            missing: vec![vec![1; 32]],
            ..ok.clone()
        },
        proto::MediaResponse {
            keep_applied: true,
            ..ok.clone()
        },
        proto::MediaResponse {
            keep_progress: Some(proto::MediaKeepProgress {
                ready: true,
                next_version: 0,
                target_current: Some(true),
            }),
            ..ok
        },
    ];
    for response in replies {
        let mut transfer = Transfer::new();
        transfer.inventory();
        let (job, _) = transfer.write();
        transfer.answer(job, Some(response));
        assert!(transfer.settle());
        transfer.available();
    }
}

fn failed_inventory(response: Option<proto::MediaResponse>) {
    let mut transfer = Transfer::new();
    let (job, request) = transfer.request();
    assert!(matches!(request, proto::media_request::Request::Have(_)));
    transfer.answer(job, response);
    assert!(transfer.settle(), "unconfirmed inventory was committed");
    transfer.available();
}

#[tokio::test]
async fn absent_or_partial_inventory_does_not_confirm_handover() {
    failed_inventory(None);
    failed_inventory(Some(proto::MediaResponse {
        status: Status::RateLimited.into(),
        ..Default::default()
    }));
    failed_inventory(Some(proto::MediaResponse {
        status: Status::Ok.into(),
        ..Default::default()
    }));
    failed_inventory(Some(proto::MediaResponse {
        status: Status::Ok.into(),
        inventory: Some(proto::MediaInventory::default()),
        ..Default::default()
    }));
}

#[tokio::test]
async fn foreign_or_duplicate_inventory_does_not_confirm_handover() {
    let part = proto::MediaPart {
        hash: dyapp_identity::sha256(b"handover bytes").to_vec(),
        index: None,
    };
    let inventories = [
        proto::MediaInventory {
            missing: vec![proto::MediaPart {
                hash: vec![1; 32],
                index: None,
            }],
            ..Default::default()
        },
        proto::MediaInventory {
            present: vec![part.clone()],
            missing: vec![part],
            ..Default::default()
        },
    ];
    for inventory in inventories {
        failed_inventory(Some(proto::MediaResponse {
            status: Status::Ok.into(),
            inventory: Some(inventory),
            ..Default::default()
        }));
    }
}

#[tokio::test]
async fn missing_holder_lookup_keeps_handover_dirty() {
    let mut transfer = Transfer::begin();
    transfer.resolve(None);
    assert!(transfer.settle());
    transfer.available();
}

#[tokio::test]
async fn deadline_with_an_outstanding_write_keeps_handover_dirty() {
    let mut transfer = Transfer::new();
    transfer.inventory();
    let _ = transfer.write();
    transfer.now = transfer.replicas.media_repairs[&transfer.id].0.deadline;
    assert!(transfer.settle());
}

#[tokio::test]
async fn acknowledged_media_write_commits_handover() {
    let mut transfer = Transfer::new();
    transfer.inventory();
    let (job, put) = transfer.write();
    let response = transfer.destination.store_media(&put).unwrap();
    assert_eq!(response.status, i32::from(Status::Ok));
    transfer.answer(job, Some(response));
    assert!(
        !transfer.settle(),
        "successfully handed over key was queued again"
    );
    transfer.available();
}
