//! Regression boundaries found by the bootstrap mutation review.
use super::*;
use crate::replication::media_encode;
use libp2p::request_response::ResponseChannel;

const WAIT: Duration = Duration::from_secs(5);

fn service() -> Service {
    service_with_clock(crate::clock::Clock::system())
}

fn service_with_clock(clock: crate::clock::Clock) -> Service {
    let dir = format!(
        "{}/../../build/test-replica-mutants-{}",
        env!("CARGO_MANIFEST_DIR"),
        uuid::Uuid::new_v4()
    );
    let mut config = crate::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    config.search.min_profile_pow_bits = 0;
    config.limits.bytes_per_second = 10_000;
    config.limits.min_free_mb = 0;
    let mut service =
        Service::with_clock(crate::BootstrapStore::new(&dir).unwrap(), config, clock).unwrap();
    service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
    service
}

async fn connect(sender: &Service, receiver: &Service) -> (Swarm<Behaviour>, Swarm<Behaviour>) {
    let mut sender_swarm = swarm(
        Keypair::generate_ed25519(),
        &sender.config.limits,
        &sender.config.roles,
    )
    .unwrap();
    let mut receiver_swarm = swarm(
        Keypair::generate_ed25519(),
        &receiver.config.limits,
        &receiver.config.roles,
    )
    .unwrap();
    receiver_swarm
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let address = tokio::time::timeout(WAIT, async {
        loop {
            if let SwarmEvent::NewListenAddr { address, .. } =
                receiver_swarm.select_next_some().await
            {
                break address;
            }
        }
    })
    .await
    .expect("loopback listener did not start");
    sender_swarm.dial(address).unwrap();
    tokio::time::timeout(WAIT, async {
        loop {
            tokio::select! {
                event = sender_swarm.select_next_some() => {
                    if matches!(event, SwarmEvent::ConnectionEstablished { .. }) {
                        break;
                    }
                }
                _ = receiver_swarm.select_next_some() => {}
            }
        }
    })
    .await
    .expect("loopback connection was not established");
    (sender_swarm, receiver_swarm)
}

async fn media_channel() -> (
    Service,
    Swarm<Behaviour>,
    Swarm<Behaviour>,
    ResponseChannel<proto::MediaResponse>,
) {
    let (sender_service, receiver_service) = (service(), service());
    let (mut sender, mut receiver) = Box::pin(connect(&sender_service, &receiver_service)).await;
    let to = *receiver.local_peer_id();
    sender.behaviour_mut().media.send_request(
        &to,
        proto::MediaRequest {
            request: Some(proto::media_request::Request::Get(proto::GetMedia {
                hash: vec![1; 32],
                len: 1,
                ..Default::default()
            })),
        },
    );
    let channel = tokio::time::timeout(WAIT, async {
        loop {
            tokio::select! {
                _ = sender.select_next_some() => {}
                event = receiver.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                        message: Message::Request { channel, .. },
                        ..
                    })) = event {
                        break channel;
                    }
                }
            }
        }
    })
    .await
    .expect("media request did not arrive");
    (receiver_service, sender, receiver, channel)
}

async fn media_response(
    sender: &mut Swarm<Behaviour>,
    receiver: &mut Swarm<Behaviour>,
) -> proto::MediaResponse {
    tokio::time::timeout(WAIT, async {
        loop {
            tokio::select! {
                event = sender.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Media(Event::Message {
                        message: Message::Response { response, .. },
                        ..
                    })) = event {
                        break response;
                    }
                }
                _ = receiver.select_next_some() => {}
            }
        }
    })
    .await
    .expect("media response did not arrive")
}

fn keep_record(owner: &dyapp_identity::Identity, hash: Vec<u8>) -> SignedRecord {
    owner.sign(
        dyapp_identity::Domain::MediaKeep,
        proto::MediaKeep {
            version: 1,
            hashes: vec![hash],
            time: crate::clock::Clock::system().timestamp().unsigned_abs(),
        }
        .encode_to_vec(),
    )
}

fn start_pending_handover(
    service: &Service,
    swarm: &mut Swarm<Behaviour>,
    replicas: &mut Replicas,
) -> (dyapp_identity::Identity, SignedRecord, Instant) {
    let owner = dyapp_identity::Identity::try_generate().unwrap();
    let profile = profile(&owner, 1, "pending");
    republish::hold_mailbox(service, 41);
    let now = republish::ready(replicas);
    replicas.scan(swarm, service, now);
    replicas.republish(swarm, service, now);
    assert_eq!(replicas.republish.len(), 1);
    (owner, profile, now)
}

fn insert_pending_states(
    service: &Service,
    replicas: &mut Replicas,
    owner: &dyapp_identity::Identity,
    reply: ResponseChannel<proto::MediaResponse>,
    now: Instant,
) {
    replicas.puts.insert(
        900,
        PendingPut::new(
            PutRecord::Media(proto::MediaReplicaPut::default()),
            None,
            false,
            now,
        ),
    );
    let repair = MediaRepair::new(
        service,
        keep_record(owner, vec![8; 32]),
        vec![vec![8; 32]],
        now,
    )
    .unwrap();
    replicas.media_repairs.insert(800, (repair, None));
    replicas.media_reads.insert(
        700,
        MediaRead {
            manifest: proto::MediaManifest::default(),
            get: proto::GetMedia::default(),
            reply,
            deadline: now + PUT_TIMEOUT,
            shards: HashMap::new(),
        },
    );
}

fn insert_pending_requests(
    swarm: &mut Swarm<Behaviour>,
    replicas: &mut Replicas,
    profile: &SignedRecord,
) -> PeerId {
    let peer = PeerId::random();
    let request = swarm
        .behaviour_mut()
        .media
        .send_request(&peer, proto::MediaRequest::default());
    replicas.mailbox_puts.insert(request, 900);
    replicas.profile_puts.insert(request, 900);
    replicas.keypackage_puts.insert(request, 900);
    replicas.media_puts.insert(request, 900);
    replicas.shard_puts.insert(request, (900, 0));
    replicas.shard_gets.insert(request, (700, 0));
    replicas.media_repair_requests.insert(request, (800, 0, 0));
    replicas.media_outgoing.insert(request, peer);
    replicas.inventories.insert(request, vec![1; 32]);
    replicas
        .profile_inventories
        .insert(request, Arc::new(profile.clone()));
    replicas.media_queue.push_back((peer, MediaJob::Put(900)));
    peer
}

fn insert_pending_upload(
    service: &Service,
    swarm: &mut Swarm<Behaviour>,
    replicas: &mut Replicas,
    owner: &dyapp_identity::Identity,
    peer: PeerId,
    now: Instant,
) {
    let conditional = owner.sign(
        dyapp_identity::Domain::MediaConditionalKeep,
        proto::MediaKeepIntent {
            version: 1,
            hashes: vec![vec![7; 32]],
            time: service.clock.timestamp().unsigned_abs(),
            requester: swarm.local_peer_id().to_string(),
            expected: Some(proto::MediaKeepCondition::default()),
            intent_hash: vec![1; 32],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    replicas.puts.insert(
        901,
        PendingPut::new(
            PutRecord::Media(proto::MediaReplicaPut {
                data: vec![7],
                authorization: Some(proto::media_replica_put::Authorization::Keep(conditional)),
                ..Default::default()
            }),
            None,
            false,
            now,
        ),
    );
    assert!(!replicas.admit_upload(swarm, service, peer, MediaJob::Put(901), now));
    assert_eq!(replicas.uploads.len(), 1);
}

#[tokio::test]
async fn pending_counts_every_replica_queue() {
    let (service, _sender, mut swarm, reply) = Box::pin(media_channel()).await;
    let mut replicas = Replicas::default();
    let (owner, profile, now) = start_pending_handover(&service, &mut swarm, &mut replicas);
    insert_pending_states(&service, &mut replicas, &owner, reply, now);
    let peer = insert_pending_requests(&mut swarm, &mut replicas, &profile);
    insert_pending_upload(&service, &mut swarm, &mut replicas, &owner, peer, now);
    let expected = replicas.puts.len()
        + replicas.mailbox_puts.len()
        + replicas.profile_puts.len()
        + replicas.keypackage_puts.len()
        + replicas.media_puts.len()
        + replicas.uploads.len()
        + replicas.shard_puts.len()
        + replicas.shard_gets.len()
        + replicas.media_reads.len()
        + replicas.media_repairs.len()
        + replicas.media_repair_requests.len()
        + replicas.media_queue.len()
        + replicas.media_outgoing.len()
        + replicas.lookups.len()
        + replicas.inventories.len()
        + replicas.profile_inventories.len()
        + replicas.republish.len();
    assert_eq!(replicas.puts.len(), 2);
    assert_eq!(replicas.mailbox_puts.len(), 1);
    assert_eq!(replicas.profile_puts.len(), 1);
    assert_eq!(replicas.keypackage_puts.len(), 1);
    assert_eq!(replicas.media_puts.len(), 1);
    assert_eq!(replicas.uploads.len(), 1);
    assert_eq!(replicas.shard_puts.len(), 1);
    assert_eq!(replicas.shard_gets.len(), 1);
    assert_eq!(replicas.media_reads.len(), 1);
    assert_eq!(replicas.media_repairs.len(), 1);
    assert_eq!(replicas.media_repair_requests.len(), 1);
    assert_eq!(replicas.media_queue.len(), 1);
    assert_eq!(replicas.media_outgoing.len(), 2);
    assert!(!replicas.lookups.is_empty());
    assert_eq!(replicas.inventories.len(), 1);
    assert_eq!(replicas.profile_inventories.len(), 1);
    assert_eq!(replicas.pending(), expected);
}

fn profile(owner: &dyapp_identity::Identity, version: u64, name: &str) -> SignedRecord {
    dyapp_profile::Profile {
        version,
        name: name.into(),
        ..Default::default()
    }
    .sign(owner)
}

fn repaired_versions(replicas: &Replicas) -> Vec<(u64, String)> {
    replicas
        .lookups
        .values()
        .map(|lookup| {
            let Lookup::ProfileRepair(record) = lookup else {
                panic!("unexpected non-profile lookup");
            };
            let profile = dyapp_profile::verify(record).unwrap().profile;
            (profile.version, profile.name)
        })
        .collect()
}

#[test]
fn profile_repair_prefers_only_a_strictly_newer_held_record() {
    let service = service();
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let owner = dyapp_identity::Identity::try_generate().unwrap();
    let incoming = profile(&owner, 2, "incoming");
    let mut replicas = Replicas::default();
    assert_eq!(
        replicas.repair_profile(&mut swarm, &service, incoming, Instant::now()),
        Vec::new()
    );
    republish::add_peer(&mut swarm);

    let held = profile(&owner, 3, "held");
    assert_eq!(
        service.store_profile(&held).unwrap().status,
        i32::from(Status::Ok)
    );
    let queries = replicas.repair_profile(
        &mut swarm,
        &service,
        profile(&owner, 2, "incoming"),
        Instant::now(),
    );
    assert_eq!(queries.len(), usize::from(REPLICAS));
    assert!(repaired_versions(&replicas)
        .iter()
        .all(|entry| entry == &(3, "held".into())));

    for (incoming, expected) in [
        (profile(&owner, 4, "incoming"), (4, "incoming".into())),
        (profile(&owner, 3, "incoming"), (3, "incoming".into())),
    ] {
        replicas.lookups.clear();
        replicas.profile_repaired.clear();
        replicas.repair_profile(&mut swarm, &service, incoming, Instant::now());
        assert!(repaired_versions(&replicas)
            .iter()
            .all(|entry| entry == &expected));
    }
}

#[tokio::test]
async fn profile_gap_sends_the_exact_owner_age() {
    let manual = crate::clock::Manual::at(1_800_000_000);
    let sender_service = service_with_clock(manual.clock());
    let receiver_service = service();
    let owner = dyapp_identity::Identity::try_generate().unwrap();
    let record = profile(&owner, 2, "age");
    assert_eq!(
        sender_service.store_profile(&record).unwrap().status,
        i32::from(Status::Ok)
    );
    manual.advance(Duration::from_secs(17));
    let (mut sender, mut receiver) = Box::pin(connect(&sender_service, &receiver_service)).await;
    let to = *receiver.local_peer_id();
    let mut replicas = Replicas::default();
    replicas.trust.add(to, 1);
    let response = proto::ProfileResponse {
        status: Status::NotFound.into(),
        ..Default::default()
    };
    assert!(replicas
        .profile_gap(&mut sender, &sender_service, to, &record, &response)
        .unwrap()
        .is_some());
    let request = tokio::time::timeout(WAIT, async {
        loop {
            tokio::select! {
                _ = sender.select_next_some() => {}
                event = receiver.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Profile(Event::Message {
                        message: Message::Request { request, .. },
                        ..
                    })) = event {
                        break request;
                    }
                }
            }
        }
    })
    .await
    .expect("profile repair request did not arrive");
    assert_eq!(request.age_seconds, 17);
    assert!(matches!(
        request.request,
        Some(proto::profile_request::Request::ReplicaPut(sent)) if sent == record
    ));
}

#[tokio::test]
async fn media_repair_admission_refuses_traffic_or_a_full_queue_independently() {
    for exhaust_queue in [false, true] {
        let service = service();
        let mut swarm = swarm(
            Keypair::generate_ed25519(),
            &service.config.limits,
            &service.config.roles,
        )
        .unwrap();
        let mut replicas = Replicas::default();
        let now = Instant::now();
        if exhaust_queue {
            for index in 0..4u8 {
                let owner = dyapp_identity::Identity::from_secret(&[index + 1; 32]);
                assert!(replicas
                    .start_media_repair(
                        &mut swarm,
                        &service,
                        keep_record(&owner, vec![index + 1; 32]),
                        now,
                    )
                    .is_some());
            }
        } else {
            service.traffic.add(10_000);
        }
        let owner = dyapp_identity::Identity::try_generate().unwrap();
        assert!(replicas
            .start_media_repair(&mut swarm, &service, keep_record(&owner, vec![9; 32]), now,)
            .is_none());
    }
}

#[tokio::test]
async fn sharded_start_put_schedules_replica_and_shard_lookups() {
    let (service, _sender, mut swarm, reply) = Box::pin(media_channel()).await;
    republish::add_peer(&mut swarm);
    let mut replicas = Replicas::default();
    replicas.start_put(
        &mut swarm,
        &service,
        &Watchers::default(),
        PutRecord::Media(proto::MediaReplicaPut {
            data: vec![4; (1 << 20) + 1],
            ..Default::default()
        }),
        PutReply::Media(reply),
    );
    let puts = replicas
        .lookups
        .values()
        .filter(|lookup| matches!(lookup, Lookup::Put(0)))
        .count();
    let shards = replicas
        .lookups
        .values()
        .filter(|lookup| matches!(lookup, Lookup::ShardPut(0, _)))
        .count();
    assert_eq!(puts, usize::from(REPLICAS));
    assert_eq!(shards, 6);
    let PutRecord::Sharded(replica, stored) = &replicas.puts[&0].record else {
        panic!("large media was not sharded");
    };
    assert_eq!(
        stored.len(),
        replica.manifest.as_ref().unwrap().shard_hashes.len()
    );
}

#[tokio::test]
async fn refused_local_shard_does_not_confirm_a_holder() {
    let service = service();
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let me = *swarm.local_peer_id();
    let mut replicas = Replicas::default();
    replicas.puts.insert(
        1,
        PendingPut::new(
            PutRecord::Sharded(proto::MediaReplicaPut::default(), Arc::new(vec![vec![1]])),
            None,
            true,
            Instant::now(),
        ),
    );
    replicas.shard_holder(&mut swarm, &service, 1, 0, me);
    assert!(replicas.puts[&1].shard_stored.is_empty());
}

fn stored_manifest(service: &Service, valid: &[usize], corrupt: &[usize]) -> proto::MediaManifest {
    let (manifest, shards) = media_encode(&vec![5; (1 << 20) + 1]).unwrap();
    let media = service.media.as_ref().unwrap();
    assert_eq!(
        media
            .keep("reader", 1, std::slice::from_ref(&manifest.hash), 1)
            .unwrap(),
        Some(vec![manifest.hash.clone()])
    );
    assert_eq!(
        media.put_manifest("reader", &manifest, 20 << 20).unwrap(),
        crate::media::Put::Stored
    );
    for &index in valid {
        assert_eq!(
            media.put_shard(&manifest, index, &shards[index]).unwrap(),
            crate::media::Put::Stored
        );
    }
    if !corrupt.is_empty() {
        let db = rusqlite::Connection::open(service.config.storage.media_path()).unwrap();
        for &index in corrupt {
            let mut broken = shards[index].clone();
            broken[0] ^= 1;
            assert_eq!(
                db.execute(
                    "UPDATE `shards` SET `data` = ? WHERE `hash` = ? AND `idx` = ?",
                    rusqlite::params![broken, manifest.hash, i64::try_from(index).unwrap()],
                )
                .unwrap(),
                1
            );
        }
    }
    manifest
}

fn read_get(manifest: &proto::MediaManifest) -> proto::GetMedia {
    proto::GetMedia {
        hash: manifest.hash.clone(),
        len: 32,
        ..Default::default()
    }
}

#[tokio::test]
async fn read_looks_up_every_missing_or_corrupt_shard() {
    let (service, _sender, mut swarm, reply) = Box::pin(media_channel()).await;
    let manifest = stored_manifest(&service, &[0, 1], &[1]);
    let now = service.clock.instant();
    let mut replicas = Replicas::default();
    replicas.start_read(
        &mut swarm,
        &service,
        manifest,
        proto::GetMedia::default(),
        reply,
    );
    let read = &replicas.media_reads[&0];
    assert_eq!(read.shards.len(), 1);
    assert!(read.shards.contains_key(&0));
    assert!(read.deadline > now);
    let mut missing: Vec<_> = replicas
        .lookups
        .values()
        .map(|lookup| {
            let Lookup::ShardGet(0, index) = lookup else {
                panic!("unexpected read lookup");
            };
            *index
        })
        .collect();
    missing.sort_unstable();
    assert_eq!(missing, [1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn exact_data_shard_count_completes_and_charges_only_the_reply_envelope() {
    let (service, mut sender, mut receiver, reply) = Box::pin(media_channel()).await;
    let manifest = stored_manifest(&service, &[0, 1], &[]);
    let get = read_get(&manifest);
    let mut replicas = Replicas::default();
    let before = service.traffic.charged_bytes();
    replicas.start_read(&mut receiver, &service, manifest, get, reply);
    assert!(replicas.media_reads.is_empty());
    assert!(replicas.lookups.is_empty());
    let response = media_response(&mut sender, &mut receiver).await;
    assert_eq!(response.status, i32::from(Status::Ok));
    assert_eq!(response.data, vec![5; 32]);
    assert_eq!(
        service.traffic.charged_bytes() - before,
        (response.encoded_len() - response.data.len()) as u64
    );
}

#[tokio::test]
async fn incomplete_read_times_out_at_the_deadline_and_cancels_lookups() {
    let (service, mut sender, mut receiver, reply) = Box::pin(media_channel()).await;
    let manifest = stored_manifest(&service, &[], &[]);
    let get = read_get(&manifest);
    let mut replicas = Replicas::default();
    replicas.start_read(&mut receiver, &service, manifest, get, reply);
    assert_eq!(replicas.lookups.len(), 6);
    let deadline = replicas.media_reads[&0].deadline;
    let before = service.traffic.charged_bytes();
    replicas.finish_reads(
        &mut receiver,
        &service,
        deadline.checked_sub(Duration::from_nanos(1)).unwrap(),
    );
    assert_eq!(replicas.media_reads.len(), 1);
    replicas.finish_reads(&mut receiver, &service, deadline);
    assert!(replicas.media_reads.is_empty());
    assert!(replicas.lookups.is_empty());
    let response = media_response(&mut sender, &mut receiver).await;
    assert_eq!(response.status, i32::from(Status::NotFound));
    assert_eq!(response.data, Vec::<u8>::new());
    assert_eq!(
        service.traffic.charged_bytes() - before,
        response.encoded_len() as u64
    );
}
