//! Conditional ownership over the real node loop, with direct admission regressions.
use dyapp_bootstrap::config::Role;
use dyapp_bootstrap::media::{KeepSnapshot, MediaStore, Put};
use dyapp_bootstrap::service::{Peer, Service};
use dyapp_bootstrap::{node, BootstrapStore, NodeConfig};
use dyapp_client::media_ownership::{Intent, Replaced};
use dyapp_client::{Error, Node};
use dyapp_identity::{sha256, Domain, Identity, SignedRecord};
use dyapp_p2p_net::proto::{self, media_request, Status};
use dyapp_p2p_net::Protocols;
use libp2p::futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::SwarmEvent;
use prost::Message;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

struct Fixture {
    service: Arc<Service>,
    owner: Identity,
    peer: Peer,
}

impl Fixture {
    fn new() -> Self {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/media-ownership-wire")
            .join(uuid::Uuid::new_v4().to_string());
        let mut config = NodeConfig::default();
        config.storage.dir.clone_from(&dir);
        config.roles = vec![Role::Store, Role::Media];
        config.network.id_pow_bits = 0;
        config.search.min_profile_pow_bits = 0;
        config.limits.requests_per_second = 10_000;
        config.limits.media_requests_per_second = 10_000;
        config.limits.min_free_mb = 0;
        let mut service = Service::new(BootstrapStore::new(dir.to_str().unwrap()).unwrap(), config);
        service.media = Some(MediaStore::open(&dir).unwrap());
        let id = Keypair::ed25519_from_bytes([23; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58();
        Self {
            service: Arc::new(service),
            owner: Identity::from_secret(&[49; 32]),
            peer: Peer {
                id,
                group: "fixture".into(),
                coarse: "fixture".into(),
            },
        }
    }

    fn media(&self) -> &MediaStore {
        self.service.media.as_ref().unwrap()
    }

    fn query(&self) -> proto::MediaKeepIntent {
        proto::MediaKeepIntent {
            time: now(),
            requester: self.peer.id.clone(),
            ..Default::default()
        }
    }

    fn write(&self, expected: &KeepSnapshot, hashes: Vec<Vec<u8>>) -> proto::MediaKeepIntent {
        proto::MediaKeepIntent {
            version: expected.version + 1,
            hashes,
            time: now(),
            intent_hash: vec![3; 32],
            pending: true,
            requester: self.peer.id.clone(),
            expected: Some(proto::MediaKeepCondition {
                owner_present: expected.owner_present,
                version: expected.version,
                record_hash: expected
                    .record
                    .as_ref()
                    .map_or_else(Vec::new, |record| sha256(&record.encode_to_vec()).to_vec()),
            }),
        }
    }

    fn signed(&self, keep: &proto::MediaKeepIntent) -> SignedRecord {
        self.owner
            .sign(Domain::MediaConditionalKeep, keep.encode_to_vec())
    }

    fn call(&self, peer: &Peer, request: media_request::Request) -> proto::MediaResponse {
        self.service
            .media(
                peer,
                proto::MediaRequest {
                    request: Some(request),
                },
            )
            .unwrap()
    }

    fn apply(&self, keep: &proto::MediaKeepIntent) -> proto::MediaResponse {
        self.call(
            &self.peer,
            media_request::Request::ConditionalKeep(self.signed(keep)),
        )
    }

    fn initial(&self) -> SignedRecord {
        let keep = self.write(&KeepSnapshot::default(), vec![sha256(b"original").to_vec()]);
        let response = self.apply(&keep);
        assert_eq!(response.status, i32::from(Status::Ok));
        assert!(response.keep_applied);
        assert_eq!(
            self.media()
                .put(&self.owner.peer_id(), b"original", 1024)
                .unwrap(),
            Put::Stored
        );
        response.keep_state.unwrap().record.unwrap()
    }
}

async fn start(service: Arc<Service>) -> Node {
    let key = Keypair::generate_ed25519();
    let peer = key.public().to_peer_id();
    let mut swarm = Box::new(
        node::swarm_with_protocols(
            key,
            &service.config.limits,
            &service.config.roles,
            service.wire.clone(),
            service.protocols.clone(),
            false,
            dyapp_p2p_net::DialFilter::default(),
            service.config.relay_hop(),
        )
        .unwrap(),
    );
    swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } = swarm.select_next_some().await {
                break address;
            }
        }
    })
    .await
    .expect("conditional media node readiness");
    swarm.add_external_address(address.clone());
    tokio::spawn(node::run(*swarm, service));
    Node::connect(
        address.with(Protocol::P2p(peer)),
        Arc::new(Protocols::public()),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn tcp_query_apply_put_and_stale_expectation_preserve_exact_intent() {
    let fixture = Fixture::new();
    let mut client = start(Arc::clone(&fixture.service)).await;
    let absent = client.media_keep_state(&fixture.owner).await.unwrap();
    assert_eq!(absent.revision(), 0);
    assert!(
        !fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap()
            .owner_present
    );
    let hash = sha256(b"original").to_vec();
    let intent = Intent {
        profile_hash: [3; 32],
        pending: true,
    };
    let Replaced::Applied { state, missing } = client
        .replace_media_keep(&fixture.owner, &absent, vec![hash.clone()], intent)
        .await
        .unwrap()
    else {
        panic!("initial ownership conflicted");
    };
    assert_eq!(missing, vec![hash.clone()]);
    assert_eq!(state.revision(), 1);
    assert_eq!(state.intent_hash(), Some([3; 32]));
    assert!(state.pending());
    state
        .record()
        .unwrap()
        .verify(Domain::MediaConditionalKeep)
        .unwrap();
    client
        .put_media(&fixture.owner.peer_id(), b"original".to_vec())
        .await
        .unwrap();
    let Replaced::Conflict(current) = client
        .replace_media_keep(&fixture.owner, &absent, Vec::new(), intent)
        .await
        .unwrap()
    else {
        panic!("stale expectation applied");
    };
    assert_eq!(current.record(), state.record());
    assert_eq!(
        fixture.media().get(&hash).unwrap(),
        Some(b"original".to_vec())
    );
}

#[tokio::test]
async fn tcp_query_neither_renews_retention_nor_changes_the_signed_record() {
    let fixture = Fixture::new();
    let old_time = now() - 3600;
    let keep = proto::MediaKeep {
        version: 4,
        hashes: vec![sha256(b"original").to_vec()],
        time: old_time,
    };
    let record = fixture.owner.sign(Domain::MediaKeep, keep.encode_to_vec());
    fixture
        .media()
        .keep_signed(&fixture.owner.peer_id(), &keep, &record)
        .unwrap();
    let mut client = start(Arc::clone(&fixture.service)).await;
    let state = client.media_keep_state(&fixture.owner).await.unwrap();
    assert_eq!(state.record(), Some(&record));
    assert_eq!(
        fixture
            .media()
            .expire_owners(i64::try_from(old_time + 1).unwrap())
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_state_cannot_cross_an_owner_switch_even_when_both_owners_are_absent() {
    let fixture = Fixture::new();
    let mut client = start(Arc::clone(&fixture.service)).await;
    let state = client.media_keep_state(&fixture.owner).await.unwrap();
    let other = Identity::from_secret(&[50; 32]);
    let result = client
        .replace_media_keep(
            &other,
            &state,
            Vec::new(),
            Intent {
                profile_hash: [3; 32],
                pending: false,
            },
        )
        .await;
    assert!(matches!(result, Err(Error::Input(_))));
    assert!(
        !fixture
            .media()
            .keep_snapshot(&other.peer_id())
            .unwrap()
            .owner_present
    );
    assert_eq!(
        client
            .media_keep_state(&fixture.owner)
            .await
            .unwrap()
            .revision(),
        0
    );
}

#[tokio::test]
async fn media_revision_is_independent_of_the_public_profiles_version_limit() {
    let fixture = Fixture::new();
    let mut client = start(Arc::clone(&fixture.service)).await;
    let state = client.media_keep_state(&fixture.owner).await.unwrap();
    let profile = dyapp_profile::Profile {
        version: u64::MAX,
        ..Default::default()
    }
    .sign(&fixture.owner);
    let digest = sha256(&profile.encode_to_vec());
    let Replaced::Applied { state, .. } = client
        .replace_media_keep(
            &fixture.owner,
            &state,
            Vec::new(),
            Intent {
                profile_hash: digest,
                pending: false,
            },
        )
        .await
        .unwrap()
    else {
        panic!("initial media ownership conflicted");
    };
    assert_eq!(state.revision(), 1);
    assert_eq!(state.intent_hash(), Some(digest));
}

#[test]
fn request_replay_from_another_transport_peer_discloses_no_state() {
    let fixture = Fixture::new();
    fixture.initial();
    let record = fixture.signed(&fixture.query());
    let other = Peer {
        id: Keypair::ed25519_from_bytes([24; 32])
            .unwrap()
            .public()
            .to_peer_id()
            .to_base58(),
        group: "other".into(),
        coarse: "other".into(),
    };
    let reply = fixture.call(&other, media_request::Request::ConditionalKeep(record));
    assert_eq!(reply.status, i32::from(Status::Denied));
    assert!(reply.keep_state.is_none());
}

#[test]
fn conditional_signatures_cannot_be_routed_through_legacy_keep_or_attach() {
    let fixture = Fixture::new();
    let keep = fixture.write(&KeepSnapshot::default(), Vec::new());
    let record = fixture.signed(&keep);
    assert!(record.verify(Domain::MediaKeep).is_err());
    for request in [
        media_request::Request::Keep(record.clone()),
        media_request::Request::Attach(record),
    ] {
        let reply = fixture.call(&fixture.peer, request);
        assert_eq!(reply.status, i32::from(Status::Denied));
        assert!(reply.keep_state.is_none());
    }
    assert!(
        !fixture
            .media()
            .keep_snapshot(&fixture.owner.peer_id())
            .unwrap()
            .owner_present
    );
}

#[test]
fn conditional_ownership_cannot_be_downgraded_by_a_newer_legacy_keep() {
    let fixture = Fixture::new();
    let initial = fixture.initial();
    let expected = fixture
        .media()
        .keep_snapshot(&fixture.owner.peer_id())
        .unwrap();
    let legacy = proto::MediaKeep {
        version: 2,
        hashes: Vec::new(),
        time: now(),
    };
    let record = fixture
        .owner
        .sign(Domain::MediaKeep, legacy.encode_to_vec());
    let reply = fixture.call(&fixture.peer, media_request::Request::Keep(record.clone()));
    assert_eq!(reply.status, i32::from(Status::Stale));
    assert!(fixture
        .media()
        .replace_keep(&fixture.owner.peer_id(), &expected, &legacy, &record)
        .is_err());
    assert_eq!(
        fixture.media().keep_of(&fixture.owner.peer_id()).unwrap(),
        Some(initial)
    );
    assert_eq!(
        fixture.media().get(&sha256(b"original")).unwrap(),
        Some(b"original".to_vec())
    );
}

#[test]
fn same_revision_replica_cannot_replace_conditional_metadata_with_a_fresher_signature() {
    let fixture = Fixture::new();
    let initial = fixture.initial();
    let mut other = proto::MediaKeepIntent::decode(initial.payload.as_slice()).unwrap();
    other.time += 1;
    other.pending = false;
    let other = fixture.signed(&other);
    let reply = fixture
        .service
        .store_media(&proto::MediaReplicaPut {
            data: b"original".to_vec(),
            authorization: Some(proto::media_replica_put::Authorization::Keep(other)),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(reply.status, i32::from(Status::Ok));
    assert_eq!(
        fixture.media().keep_of(&fixture.owner.peer_id()).unwrap(),
        Some(initial)
    );
}

#[test]
fn tampered_preconditions_and_invalid_modes_never_mutate_ownership() {
    let fixture = Fixture::new();
    let initial = fixture.initial();
    let mut tampered = fixture.signed(&fixture.query());
    tampered.payload.push(0);
    let reply = fixture.call(
        &fixture.peer,
        media_request::Request::ConditionalKeep(tampered),
    );
    assert_eq!(reply.status, i32::from(Status::Denied));
    let mut invalid = fixture.query();
    invalid.version = 1;
    assert_eq!(fixture.apply(&invalid).status, i32::from(Status::Invalid));
    let mut invalid = fixture.write(&KeepSnapshot::default(), Vec::new());
    invalid.expected.as_mut().unwrap().record_hash = vec![3; 31];
    assert_eq!(fixture.apply(&invalid).status, i32::from(Status::Invalid));
    assert_eq!(
        fixture.media().keep_of(&fixture.owner.peer_id()).unwrap(),
        Some(initial)
    );
}

#[test]
fn refreshing_a_legacy_signature_invalidates_the_wire_expectation() {
    let fixture = Fixture::new();
    let mut legacy = proto::MediaKeep {
        version: 1,
        hashes: vec![sha256(b"original").to_vec()],
        time: now() - 2,
    };
    let owner = fixture.owner.peer_id();
    let original = fixture
        .owner
        .sign(Domain::MediaKeep, legacy.encode_to_vec());
    fixture
        .media()
        .keep_signed(&owner, &legacy, &original)
        .unwrap();
    let before = fixture.media().keep_snapshot(&owner).unwrap();
    let attempted = fixture.write(&before, vec![sha256(b"replacement").to_vec()]);
    legacy.time += 1;
    let refreshed = fixture
        .owner
        .sign(Domain::MediaKeep, legacy.encode_to_vec());
    let accepted = fixture.call(
        &fixture.peer,
        media_request::Request::Keep(refreshed.clone()),
    );
    assert_eq!(accepted.status, i32::from(Status::Ok));
    let rejected = fixture.apply(&attempted);
    assert_eq!(rejected.status, i32::from(Status::Stale));
    assert!(!rejected.keep_applied);
    let current = rejected.keep_state.unwrap();
    assert_eq!(current.version, before.version);
    assert_eq!(current.record, Some(refreshed.clone()));
    let replay = fixture
        .service
        .store_media(&proto::MediaReplicaPut {
            data: b"replacement".to_vec(),
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                fixture.signed(&attempted),
            )),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(replay.status, i32::from(Status::NotFound));
    assert_eq!(fixture.media().keep_of(&owner).unwrap(), Some(refreshed));
}

#[test]
fn conditional_clock_and_record_bounds_fail_before_returning_private_state() {
    let fixture = Fixture::new();
    fixture.initial();
    for time in [now() - 3600, u64::MAX] {
        let mut query = fixture.query();
        query.time = time;
        let reply = fixture.apply(&query);
        assert_eq!(reply.status, i32::from(Status::Invalid));
        assert!(reply.server_time > 0);
        assert!(reply.keep_state.is_none());
    }
    let record = fixture
        .owner
        .sign(Domain::MediaConditionalKeep, vec![0; (1 << 20) + 1]);
    let reply = fixture.call(
        &fixture.peer,
        media_request::Request::ConditionalKeep(record),
    );
    assert_eq!(reply.status, i32::from(Status::TooLarge));
    assert!(reply.keep_state.is_none());
}
