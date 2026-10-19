//! Service tests: wire hints, profiles and replicas; mailbox and media tests are in
//! submodules.
use super::search_role::{search_reply, Applied};
use super::*;
use dyapp_identity::Identity;
use dyapp_profile::Profile;

#[test]
fn response_hints_decode_legacy_messages_and_keep_wire_tags() {
    let legacy = [0x08, Status::TooLarge as u8];
    let mut profile = ProfileResponse::decode(legacy.as_slice()).unwrap();
    let mut mailbox = MailboxResponse::decode(legacy.as_slice()).unwrap();
    let mut media = MediaResponse::decode(legacy.as_slice()).unwrap();
    let mut search = SearchResponse::decode(legacy.as_slice()).unwrap();
    assert_eq!(profile.max_bytes, 0);
    assert_eq!(mailbox.max_bytes, 0);
    assert_eq!(media.max_bytes, 0);
    assert_eq!((search.max_bytes, search.required_pow_bits), (0, 0));

    profile.max_bytes = 100;
    mailbox.max_bytes = 100;
    media.max_bytes = 100;
    search.max_bytes = 100;
    search.required_pow_bits = 11;
    let wire = |fields: &[u8]| [legacy.as_slice(), fields].concat();
    assert_eq!(profile.encode_to_vec(), wire(&[0x20, 100]));
    assert_eq!(mailbox.encode_to_vec(), wire(&[0x30, 100]));
    assert_eq!(media.encode_to_vec(), wire(&[0x28, 100]));
    assert_eq!(search.encode_to_vec(), wire(&[0x40, 100, 0x48, 11]));
    assert_eq!(
        ProfileResponse::decode(profile.encode_to_vec().as_slice()).unwrap(),
        profile
    );
    assert_eq!(
        MailboxResponse::decode(mailbox.encode_to_vec().as_slice()).unwrap(),
        mailbox
    );
    assert_eq!(
        MediaResponse::decode(media.encode_to_vec().as_slice()).unwrap(),
        media
    );
    assert_eq!(
        SearchResponse::decode(search.encode_to_vec().as_slice()).unwrap(),
        search
    );
}

#[test]
fn profile_inventory_returns_only_version_and_validates_key_and_budget() {
    let s = service_with(|limits| limits.bytes_per_second = 10_000);
    let owner = Identity::generate();
    let record = Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    s.store_profile(&record).unwrap();
    let inventory = |key| ProfileRequest {
        request: Some(profile_request::Request::Inventory(proto::GetProfile {
            peer_id: key,
        })),
    };
    let key = dyapp_identity::key_hash(&owner.public_key()).to_vec();
    let response = s
        .profile(&peer("inventory"), inventory(key.clone()))
        .unwrap();
    assert_eq!(status(&response), Status::Ok);
    assert_eq!(response.version, 2);
    assert!(response.record.is_none());
    assert_eq!(
        status(&s.profile(&peer("missing"), inventory(vec![0; 32])).unwrap()),
        Status::NotFound
    );
    assert_eq!(
        status(&s.profile(&peer("bad"), inventory(vec![0; 31])).unwrap()),
        Status::Invalid
    );
    s.traffic.add(s.config.limits.bytes_per_second);
    assert_eq!(
        status(&s.profile(&peer("busy"), inventory(key)).unwrap()),
        Status::RateLimited
    );
}

pub(super) fn search_service() -> Service {
    let mut s = service_with(|limits| limits.bytes_per_second = 1 << 20);
    s.config.roles.push(Role::Search);
    s.config.search.min_profile_pow_bits = 2;
    s.config.search.min_heartbeat_pow_bits = 0;
    let index = SearchIndex::open(&s.config.storage.search_path(), &s.config.search);
    s.search = Some(Arc::new(index.unwrap()));
    s
}

mod search;
pub(super) use search::search_as;

fn status_of(status: i32) -> Status {
    Status::try_from(status).unwrap()
}

pub(super) fn service() -> Service {
    service_with(|_| {})
}

pub(super) fn peer(id: &str) -> Peer {
    Peer {
        id: id.into(),
        group: "g".into(),
    }
}

pub(super) fn replica_peer(label: &str) -> Peer {
    let key = dyapp_identity::sha256(label.as_bytes());
    let id = libp2p::identity::Keypair::ed25519_from_bytes(key)
        .unwrap()
        .public()
        .to_peer_id()
        .to_string();
    Peer {
        id,
        group: "g".into(),
    }
}

fn service_with(change: impl FnOnce(&mut crate::config::Limits)) -> Service {
    let dir = format!("/tmp/ai/test-service-{}", uuid::Uuid::new_v4());
    let mut config = NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    change(&mut config.limits);
    Service::new(BootstrapStore::new(&dir).unwrap(), config)
}

fn publish(record: SignedRecord) -> ProfileRequest {
    ProfileRequest {
        request: Some(profile_request::Request::Publish(record)),
    }
}

pub(super) fn get(peer_id: Vec<u8>) -> ProfileRequest {
    ProfileRequest {
        request: Some(profile_request::Request::Get(proto::GetProfile { peer_id })),
    }
}

fn status(response: &ProfileResponse) -> Status {
    Status::try_from(response.status).unwrap()
}

/// A signed future-compatible profile of an exact payload size.
fn padded_profile(owner: &Identity, profile: &Profile, size: usize) -> SignedRecord {
    padded_record(owner, Domain::Profile, profile.encode_to_vec(), size)
}

fn padded_record(
    owner: &Identity,
    domain: Domain,
    mut payload: Vec<u8>,
    size: usize,
) -> SignedRecord {
    #[derive(Clone, PartialEq, prost::Message)]
    struct Extension {
        #[prost(bytes = "vec", tag = "100")]
        data: Vec<u8>,
    }
    let mut extension = Extension {
        data: vec![0; size],
    };
    let overhead = extension.encoded_len() - extension.data.len();
    extension.data.resize(size - payload.len() - overhead, 0);
    payload.extend(extension.encode_to_vec());
    assert_eq!(payload.len(), size);
    owner.sign(domain, payload)
}

#[test]
fn profile_size_hints_cover_send_replica_and_local_boundaries() {
    let s = service();
    let owner = Identity::generate();
    let profile = Profile {
        version: 1,
        age: 28,
        ..Default::default()
    };
    let exact = padded_profile(&owner, &profile, PROFILE_SEND_BYTES);
    assert!(dyapp_profile::verify(&exact).is_ok());
    let response = s.publish(&exact, false, PROFILE_SEND_BYTES - 1).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), (PROFILE_SEND_BYTES - 1) as u64)
    );
    let response = s
        .profile(&peer("profile-limit"), publish(exact.clone()))
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::Ok), 0)
    );
    let response = s.store_profile(&exact).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::Stale), 0)
    );
    let from = replica_peer("profile-limit-node");
    let response = s
        .profile(
            &from,
            ProfileRequest {
                request: Some(profile_request::Request::ReplicaPut(exact)),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::Stale), 0)
    );
    let large = padded_profile(&owner, &profile, PROFILE_SEND_BYTES + 1);
    let response = s
        .profile(&peer("profile-limit"), publish(large.clone()))
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), PROFILE_SEND_BYTES as u64)
    );
    let response = s.store_profile(&large).unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), PROFILE_ACCEPT_BYTES as u64)
    );
    let response = s
        .profile(
            &from,
            ProfileRequest {
                request: Some(profile_request::Request::ReplicaPut(large)),
            },
        )
        .unwrap();
    assert_eq!(
        (response.status, response.max_bytes),
        (i32::from(Status::TooLarge), PROFILE_ACCEPT_BYTES as u64)
    );
}

#[test]
fn publish_get_stale_and_errors() {
    let service = service();
    let identity = Identity::generate();
    let hex_id = identity.peer_id();
    let id: Vec<u8> = (0..hex_id.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_id[i..i + 2], 16).unwrap())
        .collect();
    let v2 = Profile {
        version: 2,
        age: 30,
        ..Profile::default()
    }
    .sign(&identity);

    assert_eq!(
        status(&service.profile(&peer("p"), get(id.clone())).unwrap()),
        Status::NotFound
    );
    assert_eq!(
        status(&service.profile(&peer("p"), publish(v2.clone())).unwrap()),
        Status::Ok
    );
    let got = service.profile(&peer("p"), get(id.clone())).unwrap();
    assert_eq!((status(&got), got.record), (Status::Ok, Some(v2.clone())));

    let v1 = Profile {
        version: 1,
        ..Profile::default()
    }
    .sign(&identity);
    let stale = service.profile(&peer("p"), publish(v1)).unwrap();
    assert_eq!(
        (status(&stale), stale.record),
        (Status::Stale, Some(v2.clone()))
    );

    let mut forged = v2;
    forged.payload.push(0);
    assert_eq!(
        status(&service.profile(&peer("p"), publish(forged)).unwrap()),
        Status::Denied
    );
    assert_eq!(
        status(&service.profile(&peer("p"), get(vec![1; 5])).unwrap()),
        Status::Invalid
    );
    let empty = ProfileRequest { request: None };
    assert_eq!(
        status(&service.profile(&peer("p"), empty).unwrap()),
        Status::Unsupported
    );
}

fn replica_status(service: &Service, sender: &Peer, protocol: u8) -> Status {
    let status = match protocol {
        0 => {
            service
                .mailbox(
                    sender,
                    &mut None,
                    MailboxRequest {
                        request: Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
                            envelopes: vec![SignedRecord::default()],
                            ..Default::default()
                        })),
                    },
                )
                .unwrap()
                .status
        }
        1 => {
            service
                .profile(
                    sender,
                    ProfileRequest {
                        request: Some(
                            profile_request::Request::ReplicaPut(SignedRecord::default()),
                        ),
                    },
                )
                .unwrap()
                .status
        }
        2 => {
            service
                .media(
                    sender,
                    MediaRequest {
                        request: Some(media_request::Request::ReplicaPut(proto::MediaReplicaPut {
                            data: vec![0],
                            ..Default::default()
                        })),
                    },
                )
                .unwrap()
                .status
        }
        3 => {
            service
                .media(
                    sender,
                    MediaRequest {
                        request: Some(media_request::Request::ShardPut(proto::MediaShardPut {
                            data: vec![0],
                            ..Default::default()
                        })),
                    },
                )
                .unwrap()
                .status
        }
        _ => unreachable!(),
    };
    Status::try_from(status).unwrap()
}

#[test]
fn replicas_reject_missing_pow_and_bad_signatures_with_strikes() {
    let valid = dyapp_p2p_net::generate_pow_keypair(8).public().to_peer_id();
    let invalid = loop {
        let id = libp2p::PeerId::random();
        if !dyapp_p2p_net::id_has_pow(&id, 8) {
            break id;
        }
    };
    for protocol in 0..4 {
        for sender in [
            peer("not-a-peer"),
            peer(&invalid.to_string()),
            peer(&valid.to_string()),
        ] {
            let mut service = media_service(|l| l.strikes_to_ban = 1);
            service.config.network.id_pow_bits = 8;
            assert_eq!(replica_status(&service, &sender, protocol), Status::Denied);
            assert!(service.reputation.banned(&sender.id), "protocol {protocol}");
        }
    }
}

#[test]
fn shard_upload_requires_node_pow_and_accepts_authorised_node() {
    let mut service = media_service(|_| {});
    service.config.network.id_pow_bits = 8;
    let owner = Identity::generate();
    let (manifest, shards) = crate::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
    let hash = manifest.hash.clone();
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![hash.clone()],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let put = proto::MediaShardPut {
        replica: Some(proto::MediaReplicaPut {
            manifest: Some(manifest),
            authorization: Some(proto::media_replica_put::Authorization::Keep(
                owner.sign(Domain::MediaKeep, keep.encode_to_vec()),
            )),
            ..Default::default()
        }),
        data: shards[0].clone(),
        index: 0,
    };
    let request = || MediaRequest {
        request: Some(media_request::Request::ShardPut(put.clone())),
    };
    let weak = loop {
        let id = libp2p::PeerId::random();
        if !dyapp_p2p_net::id_has_pow(&id, 8) {
            break peer(&id.to_string());
        }
    };
    assert_eq!(
        service.media(&weak, request()).unwrap().status,
        i32::from(Status::Denied)
    );
    assert!(service
        .media
        .as_ref()
        .unwrap()
        .manifest(&hash)
        .unwrap()
        .is_none());
    let node = dyapp_p2p_net::generate_pow_keypair(8).public().to_peer_id();
    assert_eq!(
        service
            .media(&peer(&node.to_string()), request())
            .unwrap()
            .status,
        i32::from(Status::Ok)
    );
    assert!(service
        .media
        .as_ref()
        .unwrap()
        .manifest(&hash)
        .unwrap()
        .is_some());
}

#[test]
fn replica_peer_group_and_media_rate_refusals_do_not_strike() {
    let sender = replica_peer("node");
    for protocol in 0..4 {
        for bucket in 0..4 {
            if bucket >= 2 && protocol < 2 {
                continue;
            }
            let mut service = media_service(|l| {
                l.strikes_to_ban = 1;
                l.requests_per_second = 1;
                l.ip_group_requests_per_second = 1;
                l.media_requests_per_second = 1;
                l.bytes_per_second = 1_000_000;
            });
            service.media_bytes = PeerRateLimiter::new(1);
            assert!(match bucket {
                0 => service.rate_limiter.check_limit(&sender.id),
                1 => service.groups.check_limit(&sender.group),
                2 => service.media_peers.check_limit(&sender.id),
                3 => service.media_bytes.check_limit(&sender.id),
                _ => unreachable!(),
            });
            assert_eq!(
                replica_status(&service, &sender, protocol),
                Status::RateLimited,
                "protocol {protocol}, bucket {bucket}"
            );
            assert!(
                !service.reputation.banned(&sender.id),
                "protocol {protocol}, bucket {bucket}"
            );
        }
    }
    // A plain client still receives a strike when the peer bucket refuses it.
    let service = service_with(|l| {
        l.strikes_to_ban = 1;
        l.requests_per_second = 1;
    });
    assert!(service.rate_limiter.check_limit("client"));
    assert_eq!(
        status(&service.profile(&peer("client"), get(vec![0; 32])).unwrap()),
        Status::RateLimited
    );
    assert!(service.reputation.banned("client"));
}

#[test]
fn admission_stores_nothing_and_replicas_verify_and_cost_one_unit() {
    let identity = Identity::generate();
    let record = Profile {
        version: 1,
        ..Profile::default()
    }
    .sign(&identity);
    let service = service_with(|l| l.requests_per_second = 5);
    assert_eq!(
        status(
            &service
                .prepare_profile(&peer("client"), record.clone())
                .unwrap()
        ),
        Status::Ok
    );
    assert!(service
        .store
        .get_profile(&identity.peer_id())
        .unwrap()
        .is_none());
    assert_eq!(
        status(&service.profile(&peer("client"), get(vec![0; 32])).unwrap()),
        Status::RateLimited
    );
    let replica = |record| ProfileRequest {
        request: Some(profile_request::Request::ReplicaPut(record)),
    };
    assert_eq!(
        status(
            &service
                .profile(&replica_peer("holder"), replica(record.clone()))
                .unwrap()
        ),
        Status::Ok
    );
    assert_eq!(
        status(
            &service
                .profile(&replica_peer("holder"), get(vec![0; 32]))
                .unwrap()
        ),
        Status::NotFound
    );
    let mut forged = record;
    forged.signature[0] ^= 1;
    assert_eq!(
        status(
            &service
                .profile(&replica_peer("holder"), replica(forged))
                .unwrap()
        ),
        Status::Denied
    );

    let envelope = proto::Envelope {
        id: vec![3; 16],
        mailbox: vec![4; 32],
        ciphertext: vec![5],
        ..Default::default()
    };
    let record = identity.sign(Domain::Envelope, envelope.encode_to_vec());
    assert_eq!(
        mailbox_status(
            &service
                .prepare_mailbox(&peer("sender"), record.clone())
                .unwrap()
        ),
        Status::Ok
    );
    assert!(service.held(&envelope.mailbox).unwrap().is_empty());
    assert_eq!(
        mailbox_status(&service.store_mailbox(&record, "local", &[]).unwrap()),
        Status::Ok
    );
    assert_eq!(service.held(&envelope.mailbox).unwrap().len(), 1);
}

#[test]
fn heartbeat_keeps_the_profile_seen() {
    let service = service();
    let identity = Identity::generate();
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    let beat = |time, identity: &Identity| {
        let beat = proto::Heartbeat {
            time,
            version: 0,
            ..Default::default()
        };
        ProfileRequest {
            request: Some(profile_request::Request::Heartbeat(
                identity.sign(Domain::Heartbeat, beat.encode_to_vec()),
            )),
        }
    };
    let send = |request| status(&service.profile(&peer("p"), request).unwrap());
    assert_eq!(send(beat(now, &identity)), Status::NotFound);
    let profile = Profile {
        version: 1,
        age: 30,
        ..Profile::default()
    };
    assert_eq!(send(publish(profile.sign(&identity))), Status::Ok);
    let db = rusqlite::Connection::open(service.config.storage.dir.join("profiles.db")).unwrap();
    let seen = || {
        db.query_row(
            "SELECT `last_seen` FROM `profiles` WHERE `peer_id` = ?",
            [identity.peer_id()],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    let old = i64::try_from(now).unwrap() - 120;
    db.execute(
        "UPDATE `profiles` SET `last_seen` = ? WHERE `peer_id` = ?",
        rusqlite::params![old, identity.peer_id()],
    )
    .unwrap();
    // Neither the same version nor an older version can refresh retention.
    assert_eq!(send(publish(profile.sign(&identity))), Status::Stale);
    let newer = Profile {
        version: 2,
        ..profile.clone()
    };
    assert_eq!(send(publish(newer.sign(&identity))), Status::Ok);
    assert!(seen() >= i64::try_from(now).unwrap());
    db.execute(
        "UPDATE `profiles` SET `last_seen` = ? WHERE `peer_id` = ?",
        rusqlite::params![old, identity.peer_id()],
    )
    .unwrap();
    assert_eq!(send(publish(profile.sign(&identity))), Status::Stale);
    assert_eq!(seen(), old);
    // Store signed time, not receipt time, and never move it back on replay.
    assert_eq!(send(beat(now - 60, &identity)), Status::Ok);
    assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
    assert_eq!(send(beat(now - 60, &identity)), Status::Ok);
    assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
    assert_eq!(send(beat(now - 90, &identity)), Status::Ok);
    assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
    assert_eq!(send(beat(now - 3600, &identity)), Status::Invalid);
    assert_eq!(send(beat(now + 3600, &identity)), Status::Invalid);
    assert_eq!(send(beat(u64::MAX, &identity)), Status::Invalid);
    assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
    let mut forged = beat(now, &identity);
    if let Some(profile_request::Request::Heartbeat(record)) = &mut forged.request {
        record.payload.push(0);
    }
    assert_eq!(send(forged), Status::Denied);
    assert_eq!(seen(), i64::try_from(now).unwrap() - 60);
    let cutoff = i64::try_from(now).unwrap() - 1;
    assert_eq!(service.store.expire_profiles(cutoff).unwrap(), 1);
}

mod mailbox;
mod write_token;
mod revoke;
mod media;
use mailbox::mailbox_status;
pub(super) use media::media_service;
