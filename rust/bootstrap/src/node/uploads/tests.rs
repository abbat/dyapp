//! Actual stores supply replies; no network latency or sleeps drive protocol transitions.
use super::*;
use crate::{BootstrapStore, NodeConfig};
use dyapp_identity::{Domain, Identity};
use libp2p::identity::Keypair;
#[path = "tests/dispatch.rs"]
mod dispatch;

fn service() -> Service {
    let dir = format!(
        "{}/../../build/upload-ancestry-{}",
        env!("CARGO_MANIFEST_DIR"),
        uuid::Uuid::new_v4()
    );
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    config.limits.min_free_mb = 0;
    config.limits.requests_per_second = 10_000;
    config.limits.media_requests_per_second = 10_000;
    config.limits.bytes_per_second = 100_000_000;
    let mut service = Service::new(BootstrapStore::new(&dir).unwrap(), config);
    service.media = Some(MediaStore::open(std::path::Path::new(&dir)).unwrap());
    service
}

fn peer() -> super::super::Peer {
    super::super::Peer {
        id: Keypair::ed25519_from_bytes([19; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_string(),
        group: "uploads".into(),
        coarse: "uploads".into(),
    }
}

fn signed(source: &Service, owner: &Identity, hash: Vec<u8>) -> SignedRecord {
    let before = source
        .media
        .as_ref()
        .unwrap()
        .keep_snapshot(&owner.peer_id())
        .unwrap();
    let record = owner.sign(
        Domain::MediaConditionalKeep,
        proto::MediaKeepIntent {
            version: before.version + 1,
            hashes: vec![hash],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
            requester: peer().id,
            expected: Some(proto::MediaKeepCondition {
                owner_present: before.owner_present,
                version: before.version,
                record_hash: before.record.map_or_else(Vec::new, |record| {
                    dyapp_identity::sha256(&record.encode_to_vec()).to_vec()
                }),
            }),
            intent_hash: vec![9; 32],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    let response = source
        .media(
            &peer(),
            proto::MediaRequest {
                request: Some(proto::media_request::Request::ConditionalKeep(
                    record.clone(),
                )),
            },
        )
        .unwrap();
    assert_eq!(response.status, i32::from(proto::Status::Ok));
    record
}

fn segment(target: &SignedRecord, ancestors: Vec<SignedRecord>) -> proto::MediaRequest {
    proto::MediaRequest {
        request: Some(proto::media_request::Request::KeepCatchup(
            proto::MediaKeepCatchup {
                target: Some(target.clone()),
                ancestors,
            },
        )),
    }
}

struct Chain {
    source: Service,
    destination: Service,
    records: Vec<SignedRecord>,
}
impl Chain {
    fn new(hash: &[u8]) -> Self {
        let source = service();
        let owner = Identity::from_secret(&[62; 32]);
        let records = (0u8..4)
            .map(|version| {
                let hash = if version == 0 || version == 3 {
                    hash.to_vec()
                } else {
                    dyapp_identity::sha256(&[version]).to_vec()
                };
                signed(&source, &owner, hash)
            })
            .collect::<Vec<_>>();
        let destination = service();
        let response = destination
            .media(&peer(), segment(&records[0], vec![]))
            .unwrap();
        assert_eq!(response.status, i32::from(proto::Status::Ok));
        Self {
            source,
            destination,
            records,
        }
    }

    fn target(&self) -> &SignedRecord {
        self.records.last().unwrap()
    }

    fn reply(&self, request: proto::MediaRequest) -> proto::MediaResponse {
        self.destination.media(&peer(), request).unwrap()
    }
}

#[test]
fn ordinary_upload_admission_uses_real_bounded_history_without_intermediate_bytes() {
    let chain = Chain::new(&dyapp_identity::sha256(b"upload"));
    let remote = PeerId::random();
    let mut uploads = Uploads::default();
    let mut pages = Vec::new();
    loop {
        match uploads.gate(
            1,
            remote,
            chain.target(),
            chain.source.media.as_ref().unwrap(),
        ) {
            Gate::Request(request) => {
                let Some(proto::media_request::Request::KeepCatchup(ref page)) = request.request
                else {
                    panic!("expected ancestry");
                };
                pages.push(page.ancestors.len());
                assert!(uploads.receive(1, remote, Some(chain.reply(request))));
            }
            Gate::Send => break,
            _ => panic!("admission did not make progress"),
        }
        assert!(pages.len() <= 2, "repeated progress without completion");
    }
    assert_eq!(pages, vec![0, 3]);
    let owner = chain.target().verify(Domain::MediaConditionalKeep).unwrap();
    assert_eq!(
        chain
            .destination
            .media
            .as_ref()
            .unwrap()
            .keep_snapshot(&owner)
            .unwrap()
            .record
            .as_ref(),
        Some(chain.target())
    );
}

#[test]
fn missing_required_proof_fails_but_optional_history_can_end_after_admission() {
    let data = b"upload";
    let chain = Chain::new(&dyapp_identity::sha256(data));
    let seeded = service();
    let replica = proto::MediaReplicaPut {
        data: data.to_vec(),
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            chain.target().clone(),
        )),
        ..Default::default()
    };
    assert_eq!(
        seeded.store_media(&replica).unwrap().status,
        i32::from(proto::Status::Ok)
    );
    for lagging in [true, false] {
        let fresh = service();
        let destination = if lagging { &chain.destination } else { &fresh };
        let remote = PeerId::random();
        let mut uploads = Uploads::default();
        let Gate::Request(probe) =
            uploads.gate(1, remote, chain.target(), seeded.media.as_ref().unwrap())
        else {
            panic!("expected probe");
        };
        let reply = destination.media(&peer(), probe).unwrap();
        assert!(uploads.receive(1, remote, Some(reply)));
        let next = uploads.gate(1, remote, chain.target(), seeded.media.as_ref().unwrap());
        if lagging {
            assert!(matches!(next, Gate::Drop));
        } else {
            assert!(matches!(next, Gate::Send));
        }
    }
}

#[test]
fn actual_legacy_signature_bypasses_admission_but_corruption_does_not() {
    let source = service();
    let owner = Identity::from_secret(&[63; 32]);
    let mut record = owner.sign(
        Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![dyapp_identity::sha256(b"legacy").to_vec()],
            time: chrono::Utc::now().timestamp().unsigned_abs(),
        }
        .encode_to_vec(),
    );
    let mut uploads = Uploads::default();
    let remote = PeerId::random();
    assert!(matches!(
        uploads.gate(1, remote, &record, source.media.as_ref().unwrap()),
        Gate::Send
    ));
    record.payload.push(0);
    assert!(matches!(
        uploads.gate(2, remote, &record, source.media.as_ref().unwrap()),
        Gate::Drop
    ));
}

#[test]
fn duplicate_parts_wait_for_one_admission_and_fail_closed_after_refusal() {
    let chain = Chain::new(&dyapp_identity::sha256(b"upload"));
    let remote = PeerId::random();
    let mut uploads = Uploads::default();
    assert!(matches!(
        uploads.gate(
            1,
            remote,
            chain.target(),
            chain.source.media.as_ref().unwrap()
        ),
        Gate::Request(_)
    ));
    assert!(matches!(
        uploads.gate(
            1,
            remote,
            chain.target(),
            chain.source.media.as_ref().unwrap()
        ),
        Gate::Wait
    ));
    assert!(!uploads.receive(
        1,
        remote,
        Some(proto::MediaResponse {
            status: proto::Status::Unsupported.into(),
            ..Default::default()
        })
    ));
    assert!(matches!(
        uploads.gate(
            1,
            remote,
            chain.target(),
            chain.source.media.as_ref().unwrap()
        ),
        Gate::Drop
    ));
}
