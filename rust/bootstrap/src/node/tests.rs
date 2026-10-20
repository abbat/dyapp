//! Node tests.
use super::*;

#[test]
fn catch_up_asks_two_distinct_peers_per_topic() {
    let peers: Vec<PeerId> = (0..3).map(|_| PeerId::random()).collect();
    assert_eq!(catch_up_peers(&peers, u32::MAX), [peers[0], peers[1]]);
    assert_eq!(catch_up_peers(&peers[..1], 7), [peers[0]]);
    assert!(catch_up_peers(&[], 7).is_empty());
}

#[tokio::test]
async fn hourly_status_reports_and_resets_wire_bytes_and_failed_gossip_adds_none() {
    let dir = format!("/tmp/ai/test-wire-status-{}", uuid::Uuid::new_v4());
    let mut config = crate::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    let service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config);
    let mut swarm = swarm_with_wire(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &[Role::Store, Role::Search],
        service.wire.clone(),
    )
    .unwrap();
    for status in [Status::Invalid, Status::Ok] {
        spread(
            &mut swarm,
            Some(([0; 32], vec![1; 7])),
            &SearchResponse {
                status: status.into(),
                ..Default::default()
            },
            &service,
        );
    }
    assert_eq!(service.wire.take().0, [[0; 2]; 11]);
    service.wire.add(0, dyapp_p2p_net::Dir::In, 7);
    service.wire.add(0, dyapp_p2p_net::Dir::Out, 11);
    let path = format!("{dir}/status.log");
    let writer_path = path.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_writer(move || {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&writer_path)
                .unwrap()
        })
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        status(&service, &swarm, 0, 3, 0, 0, 0, 0);
        status(&service, &swarm, 0, 0, 0, 0, 0, 0);
    });
    let log = std::fs::read_to_string(path).unwrap();
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("wire_bytes=node=7/11 profile=0/0"));
    assert!(lines[1].contains("wire_bytes=node=0/0 profile=0/0"));
    assert!(lines[0].contains("answered=3"));
    assert!(lines[0].contains("pow_checks=0"));
    assert_eq!(service.wire.take().0, [[0; 2]; 11]);
}

mod groups;

#[tokio::test]
async fn media_queue_drops_capped_writes_without_tracking_or_trust_penalties() {
    let dir = format!("/tmp/ai/test-media-cap-{}", uuid::Uuid::new_v4());
    let mut config = crate::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    let mut service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config);
    service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let owner = dyapp_identity::Identity::generate();
    let data = vec![1];
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![dyapp_identity::sha256(&data).to_vec()],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let record = proto::MediaReplicaPut {
        data,
        authorization: Some(proto::media_replica_put::Authorization::Keep(
            owner.sign(dyapp_identity::Domain::MediaKeep, keep.encode_to_vec()),
        )),
        ..Default::default()
    };
    let mut replicas = Replicas {
        cap: ReplicaCap::new(1),
        ..Default::default()
    };
    let to = PeerId::random();
    replicas.trust.add(to, 3);
    assert!(replicas.cap.allow(&to, 1));
    replicas.puts.insert(
        1,
        PendingPut::new(PutRecord::Media(record.clone()), None, false),
    );
    replicas.puts.insert(
        2,
        PendingPut::new(
            PutRecord::Sharded(record, vec![vec![1]].into()),
            None,
            false,
        ),
    );
    replicas.put_holder(&mut swarm, &service, &Watchers::default(), 1, to);
    replicas.shard_holder(&mut swarm, &service, 2, 0, to);
    assert_eq!(replicas.media_queue.len(), 2);
    replicas.drain_media(&mut swarm, &service);
    assert!(replicas.media_queue.is_empty());
    assert!(replicas.media_outgoing.is_empty());
    assert!(replicas.media_puts.is_empty());
    assert!(replicas.shard_puts.is_empty());
    assert!(replicas.puts[&1].stored.is_empty());
    assert!(replicas.puts[&2].shard_stored.is_empty());
    assert_eq!(replicas.puts[&1].result(Instant::now()), None);
    assert_eq!(
        replicas.puts[&1].result(replicas.puts[&1].deadline),
        Some(Status::Full)
    );
    assert_eq!(replicas.cap.take_dropped(), 2);
    assert_eq!(replicas.trust.score[&to], 3);
    for read in [
        proto::media_request::Request::Get(proto::GetMedia::default()),
        proto::media_request::Request::ShardGet(proto::MediaShardGet::default()),
        proto::media_request::Request::Have(proto::MediaHave::default()),
    ] {
        assert!(replicas.cap.allow_media(&to, &read));
    }
    assert_eq!(replicas.cap.take_dropped(), 0);
    let me = *swarm.local_peer_id();
    assert!(replicas.cap.allow(&me, 1));
    replicas.put_holder(&mut swarm, &service, &Watchers::default(), 1, me);
    assert!(replicas.puts[&1].stored.contains(&me));
    assert_eq!(replicas.cap.take_dropped(), 0);
    assert_eq!(
        service
            .media
            .as_ref()
            .unwrap()
            .get(&keep.hashes[0])
            .unwrap(),
        Some(vec![1])
    );
}

async fn receive_repair_batch(
    sender: &mut Swarm<Behaviour>,
    receiver: &mut Swarm<Behaviour>,
    service: &Service,
) -> usize {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                _ = sender.select_next_some() => {}
                event = receiver.select_next_some() => {
                    if let SwarmEvent::Behaviour(BehaviourEvent::Mailbox(Event::Message {
                        peer,
                        message: Message::Request { request, channel, .. },
                        ..
                    })) = event {
                        let Some(mailbox_request::Request::ReplicaPut(batch)) = &request.request else {
                            panic!("expected repair batch");
                        };
                        let count = batch.envelopes.len();
                        assert!((1..=50).contains(&count));
                        assert!(request.encoded_len() <= REPAIR_BYTES + 64 * 1024);
                        let response = service.mailbox(
                            &Peer { id: peer.to_string(), group: "g".into() },
                            &mut None,
                            request,
                        ).unwrap();
                        assert_eq!(response.status, i32::from(Status::Ok));
                        receiver.behaviour_mut().mailbox.send_response(channel, response).unwrap();
                        return count;
                    }
                }
            }
        }
    })
    .await
    .expect("repair batch never arrived")
}

#[tokio::test]
async fn mailbox_repair_over_fifty_envelopes_makes_progress_after_refill() {
    let service = || {
        let dir = format!("/tmp/ai/test-repair-cap-{}", uuid::Uuid::new_v4());
        let mut config = crate::config::NodeConfig::default();
        config.storage.dir = dir.clone().into();
        config.network.id_pow_bits = 0;
        config.limits.sender_puts_per_second = 1000;
        Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config)
    };
    let (source, destination) = (service(), service());
    let mut sender = swarm(
        Keypair::generate_ed25519(),
        &source.config.limits,
        &source.config.roles,
    )
    .unwrap();
    let mut receiver = swarm(
        Keypair::generate_ed25519(),
        &destination.config.limits,
        &destination.config.roles,
    )
    .unwrap();
    let remote = *receiver.local_peer_id();
    receiver
        .listen_on("/ip4/127.0.0.1/tcp/0".parse().unwrap())
        .unwrap();
    let addr = loop {
        if let SwarmEvent::NewListenAddr { address, .. } = receiver.select_next_some().await {
            break address;
        }
    };
    // By address only: the peer-proof gate refuses a known peer ID without node-ID proof of work.
    sender.dial(addr).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                event = sender.select_next_some() => {
                    if matches!(event, SwarmEvent::ConnectionEstablished { .. }) { break; }
                }
                _ = receiver.select_next_some() => {}
            }
        }
    })
    .await
    .unwrap();
    let owner = dyapp_identity::Identity::generate();
    let mailbox = vec![2; 32];
    for i in 0..75u8 {
        let record = owner.sign(
            dyapp_identity::Domain::Envelope,
            proto::Envelope {
                id: vec![i; 16],
                mailbox: mailbox.clone(),
                ciphertext: vec![3],
                ..Default::default()
            }
            .encode_to_vec(),
        );
        assert_eq!(
            source.store_mailbox(&record, "local", &[]).unwrap().status,
            i32::from(Status::Ok)
        );
    }
    let mut cap = ReplicaCap::default();
    send_envelopes(&mut sender, &source, &mut cap, remote, &mailbox, |_| true);
    assert_eq!(cap.take_dropped(), 1, "only the second batch is refused");
    assert_eq!(
        receive_repair_batch(&mut sender, &mut receiver, &destination).await,
        50
    );
    let held = destination.held(&mailbox).unwrap();
    assert_eq!(held.len(), 50);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    send_envelopes(&mut sender, &source, &mut cap, remote, &mailbox, |id| {
        !held.contains_key(id)
    });
    assert_eq!(
        receive_repair_batch(&mut sender, &mut receiver, &destination).await,
        25
    );
    assert_eq!(destination.held(&mailbox).unwrap().len(), 75);
    assert_eq!(cap.take_dropped(), 0);
    // Inventory of the complete mailbox sends nothing and consumes no units.
    send_envelopes(&mut sender, &source, &mut cap, remote, &mailbox, |_| false);
    assert!(cap.allow(&remote, 25));
}

#[tokio::test]
async fn repair_sends_no_signals_but_lists_them() {
    let dir = format!("/tmp/ai/test-repair-signals-{}", uuid::Uuid::new_v4());
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    config.limits.sender_puts_per_second = 1000;
    let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
    let owner = dyapp_identity::Identity::generate();
    let mailbox = vec![2; 32];
    for i in 0..4u8 {
        let class = if i == 0 {
            proto::EnvelopeClass::Message
        } else {
            proto::EnvelopeClass::Signal
        };
        let envelope = proto::Envelope {
            id: vec![i; 16],
            mailbox: mailbox.clone(),
            ciphertext: vec![3],
            class: class.into(),
            ..Default::default()
        };
        let record = owner.sign(dyapp_identity::Domain::Envelope, envelope.encode_to_vec());
        let response = service.store_mailbox(&record, "local", &[]).unwrap();
        assert_eq!(response.status, i32::from(Status::Ok));
    }
    let held = service.held(&mailbox).unwrap();
    assert_eq!(held.len(), 4, "the inventory lists signals");
    let (limits, roles) = (&service.config.limits, &service.config.roles);
    let mut sender = swarm(Keypair::generate_ed25519(), limits, roles).unwrap();
    // A one-unit burst admits the single message; a batch carrying the signals would be refused.
    let mut cap = ReplicaCap::new(1);
    let to = PeerId::random();
    send_envelopes(&mut sender, &service, &mut cap, to, &mailbox, |_| true);
    assert_eq!(cap.take_dropped(), 0);
}

#[test]
fn replica_cap_has_atomic_burst_separate_destinations_and_refill() {
    let mut cap = ReplicaCap::default();
    let (a, b) = (PeerId::random(), PeerId::random());
    assert!(!cap.allow(&a, 51), "a batch cannot exceed the burst");
    assert!(cap.allow(&a, 50), "failed charge must spend nothing");
    assert!(!cap.allow(&a, 50));
    assert!(cap.allow(&b, 50));
    assert_eq!(cap.take_dropped(), 2);
    assert_eq!(cap.take_dropped(), 0);
    assert!(
        !cap.allow(&a, 50),
        "taking counts must not refill the bucket"
    );
    std::thread::sleep(Duration::from_millis(1100));
    assert!(cap.allow(&a, 50));
    assert_eq!(cap.take_dropped(), 1);
}

#[tokio::test]
async fn mailbox_profile_and_profile_repair_share_the_sender_cap() {
    let dir = format!("/tmp/ai/test-replica-cap-{}", uuid::Uuid::new_v4());
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let mut replicas = Replicas {
        cap: ReplicaCap::new(2),
        ..Default::default()
    };
    let watchers = Watchers::default();
    let remote = PeerId::random();
    replicas.trust.add(remote, 5);
    let owner = dyapp_identity::Identity::generate();
    let record = dyapp_profile::Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    replicas.puts.insert(
        0,
        PendingPut::new(
            PutRecord::Mailbox(SignedRecord::default(), vec![]),
            None,
            false,
        ),
    );
    replicas.puts.insert(
        1,
        PendingPut::new(PutRecord::Profile(record.clone()), None, false),
    );
    replicas.put_holder(&mut swarm, &service, &watchers, 0, remote);
    assert_eq!(replicas.mailbox_puts.len(), 1);
    let missing = proto::ProfileResponse {
        status: Status::NotFound.into(),
        ..Default::default()
    };
    assert!(replicas
        .profile_gap(&mut swarm, &service, remote, &record, &missing)
        .is_some());
    replicas.put_holder(&mut swarm, &service, &watchers, 1, remote);
    assert!(replicas.profile_puts.is_empty());
    assert!(replicas.puts[&1].sent.contains(&remote));
    assert!(replicas.puts[&1].stored.is_empty());
    assert_eq!(replicas.puts[&1].result(Instant::now()), None);
    assert_eq!(
        replicas.puts[&1].result(replicas.puts[&1].deadline),
        Some(Status::Full)
    );
    assert_eq!(replicas.cap.take_dropped(), 1);
    replicas.put_holder(&mut swarm, &service, &watchers, 1, remote);
    assert_eq!(
        replicas.cap.take_dropped(),
        0,
        "duplicate holder is not retried"
    );
    assert_eq!(replicas.trust.score[&remote], 5);
    assert!(replicas.media_queue.is_empty());
    let other = PeerId::random();
    replicas.put_holder(&mut swarm, &service, &watchers, 1, other);
    assert_eq!(replicas.profile_puts.len(), 1);
    // A negative trust score prevents sends before the cap is charged.
    let untrusted = PeerId::random();
    replicas.trust.add(untrusted, -1);
    replicas.put_holder(&mut swarm, &service, &watchers, 1, untrusted);
    assert!(replicas.cap.allow(&untrusted, 2));

    let me = *swarm.local_peer_id();
    assert!(replicas.cap.allow(&me, 2));
    replicas.put_holder(&mut swarm, &service, &watchers, 1, me);
    assert!(replicas.puts[&1].stored.contains(&me));
    assert_eq!(
        replicas.cap.take_dropped(),
        0,
        "local write bypasses exhausted cap"
    );
    assert!(service
        .store
        .get_profile(&owner.peer_id())
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn media_inventory_requires_near_trusted_peers_and_budget() {
    let dir = format!("/tmp/ai/test-media-inventory-{}", uuid::Uuid::new_v4());
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.limits.bytes_per_second = 10_000;
    let mut service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
    service.media = Some(crate::media::MediaStore::open(Path::new(&dir)).unwrap());
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let near = PeerId::random();
    let far = PeerId::random();
    swarm
        .behaviour_mut()
        .kad
        .add_address(&near, "/ip4/127.0.0.1/tcp/1".parse().unwrap());
    let items = vec![proto::MediaPart {
        hash: vec![1; 32],
        index: None,
    }];
    let mut replicas = Replicas::default();
    let reply = replicas
        .media_inventory(&mut swarm, &service, far, &items)
        .unwrap()
        .inventory
        .unwrap();
    assert!(
        reply.present.is_empty()
            && reply.missing.is_empty()
            && reply.manifests.is_empty()
            && reply.whole.is_empty()
    );
    replicas.trust.score.insert(near, -1);
    assert!(replicas
        .media_inventory(&mut swarm, &service, near, &items)
        .unwrap()
        .inventory
        .unwrap()
        .missing
        .is_empty());
    replicas.trust.score.insert(near, 0);
    assert_eq!(
        replicas
            .media_inventory(&mut swarm, &service, near, &items)
            .unwrap()
            .inventory
            .unwrap()
            .missing,
        items
    );
    service.traffic.add(10_000);
    assert_eq!(
        replicas
            .media_inventory(&mut swarm, &service, near, &items)
            .unwrap()
            .status,
        i32::from(Status::RateLimited)
    );
}

#[tokio::test]
async fn profile_repair_only_fills_gaps_once_an_hour_within_budget() {
    let dir = format!("/tmp/ai/test-profile-repair-{}", uuid::Uuid::new_v4());
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.limits.bytes_per_second = 10_000;
    let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let remote = PeerId::random();
    swarm
        .behaviour_mut()
        .kad
        .add_address(&remote, "/ip4/127.0.0.1/tcp/1".parse().unwrap());
    let owner = dyapp_identity::Identity::generate();
    let record = dyapp_profile::Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    let key = dyapp_identity::key_hash(&owner.public_key()).to_vec();
    let mut replicas = Replicas::default();
    let now = Instant::now();
    replicas.repair_profile(&mut swarm, &service, record.clone(), now);
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    replicas.repair_profile(
        &mut swarm,
        &service,
        record.clone(),
        now + PROFILE_REPAIR_INTERVAL - Duration::from_nanos(1),
    );
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    replicas.lookups.clear();
    replicas.repair_profile(
        &mut swarm,
        &service,
        record.clone(),
        now + PROFILE_REPAIR_INTERVAL,
    );
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    assert_eq!(
        replicas.profile_repaired[&key].0,
        now + PROFILE_REPAIR_INTERVAL
    );
    for _ in 0..REPLICAS {
        replicas.profile_inventory(&mut swarm, &service, remote, Arc::new(record.clone()));
    }
    assert_eq!(
        replicas.profile_inventories.len(),
        1,
        "co-located replicas are inventoried once"
    );
    for (status, version, sends) in [
        (Status::Ok, 1, true),
        (Status::NotFound, 0, true),
        (Status::Ok, 2, false),
        (Status::Ok, 3, false),
        (Status::Denied, 0, false),
        (Status::RateLimited, 0, false),
    ] {
        let response = proto::ProfileResponse {
            status: status.into(),
            version,
            ..Default::default()
        };
        assert_eq!(
            replicas
                .profile_gap(&mut swarm, &service, remote, &record, &response)
                .is_some(),
            sends
        );
    }
    service.traffic.add(10_000);
    let response = proto::ProfileResponse {
        status: Status::NotFound.into(),
        ..Default::default()
    };
    assert!(replicas
        .profile_gap(&mut swarm, &service, remote, &record, &response)
        .is_none());
    let other = PeerId::random();
    replicas.profile_inventory(&mut swarm, &service, other, Arc::new(record));
    assert_eq!(
        replicas.profile_inventories.len(),
        1,
        "repair exceeded its budget"
    );
}

#[test]
fn put_ack_counts_distinct_holders_and_times_out() {
    let record = PutRecord::Mailbox(SignedRecord::default(), vec![]);
    let (a, b) = (PeerId::random(), PeerId::random());
    let mut put = PendingPut::new(record.clone(), None, false);
    put.confirm(a, Status::Ok.into(), None);
    put.confirm(a, Status::Ok.into(), None);
    put.confirm(b, Status::Denied.into(), None);
    assert_eq!(put.result(Instant::now()), None);
    assert_eq!(put.result(put.deadline), Some(Status::Full));
    put.confirm(b, Status::Ok.into(), None);
    assert_eq!(put.result(Instant::now()), Some(Status::Ok));
    assert_eq!(
        put.result(put.deadline),
        Some(Status::Full),
        "late replies beat the deadline"
    );
    let mut isolated = PendingPut::new(record, None, true);
    isolated.confirm(a, Status::Ok.into(), None);
    assert_eq!(isolated.result(Instant::now()), Some(Status::Ok));
}

#[tokio::test]
async fn sharded_writes_require_k_indices_and_queue_two_streams_per_peer() {
    let (manifest, shards) = crate::replication::media_encode(&vec![1; (1 << 20) + 1]).unwrap();
    let replica = proto::MediaReplicaPut {
        manifest: Some(manifest),
        ..Default::default()
    };
    let mut put = PendingPut::new(PutRecord::Sharded(replica, Arc::new(shards)), None, false);
    put.confirm(PeerId::random(), Status::Ok.into(), None);
    put.confirm(PeerId::random(), Status::Ok.into(), None);
    put.shard_stored.insert(0);
    put.shard_stored.insert(0);
    assert_eq!(put.result(Instant::now()), None);
    put.shard_stored.insert(1);
    assert_eq!(put.result(Instant::now()), Some(Status::Ok));
    let service = Service::new(
        crate::BootstrapStore::new(&format!(
            "/tmp/ai/test-shard-queue-{}",
            uuid::Uuid::new_v4()
        ))
        .unwrap(),
        crate::NodeConfig::default(),
    );
    let mut swarm = dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    let mut replicas = Replicas::default();
    replicas.puts.insert(1, put);
    let to = PeerId::random();
    for index in 0..6 {
        replicas.shard_holder(&mut swarm, &service, 1, index, to);
    }
    replicas.drain_media(&mut swarm, &service);
    assert_eq!(replicas.media_outgoing.len(), 2);
    assert_eq!(replicas.media_queue.len(), 4);
    let finished = *replicas.shard_puts.keys().next().unwrap();
    replicas.media_outgoing.remove(&finished);
    replicas.shard_puts.remove(&finished);
    replicas.drain_media(&mut swarm, &service);
    assert_eq!(replicas.media_outgoing.len(), 2);
    assert_eq!(replicas.media_queue.len(), 3);
}

#[tokio::test]
async fn media_holder_status_counts_distinct_confirmations_for_the_latest_write() {
    let service = Service::new(
        crate::BootstrapStore::new(&format!(
            "/tmp/ai/test-media-holders-{}",
            uuid::Uuid::new_v4()
        ))
        .unwrap(),
        crate::NodeConfig::default(),
    );
    let mut swarm = dyapp_p2p_net::build_swarm(Keypair::generate_ed25519(), Mode::Auto).unwrap();
    let mut replicas = Replicas::default();
    let a = PeerId::random();
    let b = PeerId::random();
    let mut put = PendingPut::new(
        PutRecord::Media(proto::MediaReplicaPut::default()),
        None,
        false,
    );
    put.confirm(a, Status::Ok.into(), None);
    put.confirm(a, Status::Ok.into(), None);
    put.confirm(b, Status::Full.into(), None);
    replicas.puts.insert(1, put);
    // Retain the put as background work while status takes its snapshot.
    replicas.media_puts.insert(
        swarm
            .behaviour_mut()
            .media
            .send_request(&a, proto::MediaRequest::default()),
        1,
    );
    replicas.finish_puts(&mut swarm, &service, Instant::now());
    assert_eq!(replicas.media_holders, (1, 1));
    replicas
        .puts
        .get_mut(&1)
        .unwrap()
        .confirm(b, Status::Ok.into(), None);
    replicas.finish_puts(&mut swarm, &service, Instant::now());
    assert_eq!(replicas.media_holders, (1, 2));
    let mut latest = PendingPut::new(
        PutRecord::Media(proto::MediaReplicaPut::default()),
        None,
        true,
    );
    latest.confirm(a, Status::Ok.into(), None);
    replicas.puts.insert(2, latest);
    replicas.finish_puts(&mut swarm, &service, Instant::now());
    assert_eq!(
        replicas.media_holders,
        (2, 1),
        "older writes cannot inflate the latest count"
    );
}

#[test]
fn stale_profile_confirmation_must_verify_the_owner_and_version() {
    let owner = dyapp_identity::Identity::generate();
    let sign = |version| {
        dyapp_profile::Profile {
            version,
            ..Default::default()
        }
        .sign(&owner)
    };
    let mut put = PendingPut::new(PutRecord::Profile(sign(2)), None, false);
    let (a, b) = (PeerId::random(), PeerId::random());
    put.confirm(a, Status::Stale.into(), Some(sign(1)));
    let mut forged = sign(3);
    forged.signature[0] ^= 1;
    put.confirm(a, Status::Stale.into(), Some(forged));
    assert!(put.stored.is_empty());
    put.confirm(a, Status::Stale.into(), Some(sign(3)));
    put.confirm(b, Status::Stale.into(), Some(sign(2)));
    assert_eq!(put.result(Instant::now()), Some(Status::Stale));
    assert_eq!(put.stale, Some(sign(3)));
}

#[tokio::test]
async fn holders_store_once_and_replica_puts_never_fan_out() {
    let dir = format!("/tmp/ai/test-fanout-{}", uuid::Uuid::new_v4());
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    let service = Service::new(crate::storage::BootstrapStore::new(&dir).unwrap(), config);
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let me = *swarm.local_peer_id();
    let mut replicas = Replicas::default();
    let mut watchers = Watchers::new();
    let sender = dyapp_identity::Identity::generate();
    let record = sender.sign(
        dyapp_identity::Domain::Envelope,
        proto::Envelope {
            id: vec![1; 16],
            mailbox: vec![2; 32],
            ciphertext: vec![3],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    replicas.puts.insert(
        0,
        PendingPut::new(PutRecord::Mailbox(record.clone(), vec![]), None, false),
    );
    replicas.put_holder(&mut swarm, &service, &watchers, 0, PeerId::random());
    assert_eq!(replicas.mailbox_puts.len(), 1);
    assert!(
        service.held(&[2; 32]).unwrap().is_empty(),
        "a forwarding acceptor stored a copy"
    );
    for _ in 0..REPLICAS {
        replicas.put_holder(&mut swarm, &service, &watchers, 0, me);
    }
    assert_eq!(service.held(&[2; 32]).unwrap().len(), 1);
    assert_eq!(replicas.puts[&0].stored.len(), 1);

    let connection = ConnectionId::new_unchecked(1);
    after_mailbox(
        &mut swarm,
        &service,
        &mut watchers,
        &mut replicas,
        (PeerId::random(), connection),
        Some(mailbox_request::Request::ReplicaPut(proto::Envelopes {
            envelopes: vec![record.clone()],
            ..Default::default()
        })),
    );
    assert!(replicas.lookups.is_empty());
    assert_eq!(
        replicas.mailbox_puts.len(),
        1,
        "replica_put forwarded again"
    );
    for id in 1..MAX_PENDING_PUTS as u64 {
        replicas.puts.insert(
            id,
            PendingPut::new(PutRecord::Mailbox(record.clone(), vec![]), None, false),
        );
    }
    assert!(replicas.at_capacity());
    replicas.finish_puts(&mut swarm, &service, Instant::now() + PUT_TIMEOUT);
    assert!(!replicas.at_capacity() && replicas.puts.is_empty());
}

#[test]
fn holder_selection_includes_local_and_handles_unsorted_lookup_results() {
    let me = PeerId::random();
    let nodes: Vec<_> = (0..8)
        .map(|_| kad::PeerInfo {
            peer_id: PeerId::random(),
            addrs: vec![],
        })
        .collect();
    assert_eq!(holder(me, &me.to_bytes(), &nodes), me);
    for i in 0..128u8 {
        let key = replica_key(&[i; 32], 0);
        let selected = holder(me, &key, &nodes);
        let target = kad::KBucketKey::new(key.clone());
        let distance = kad::KBucketKey::from(selected).distance(&target);
        assert!(nodes
            .iter()
            .all(|n| distance <= kad::KBucketKey::from(n.peer_id).distance(&target)));
        assert!(distance <= kad::KBucketKey::from(me).distance(&target));
    }
    assert_eq!(holder(me, &[1], &[]), me);
}

#[test]
fn damaged_peer_cache_lines_are_skipped() {
    let dir = std::env::temp_dir().join(format!("dyapp-peers-{}", std::process::id()));
    assert!(cached_peers(&dir).is_empty());
    fs::write(&dir, "/ip4/1.2.3.4/tcp/1\ngarbage\n").unwrap();
    assert_eq!(cached_peers(&dir), ["/ip4/1.2.3.4/tcp/1".parse().unwrap()]);
    fs::remove_file(&dir).unwrap();
}

#[test]
fn replicas_go_to_peers_seen_long_enough_that_answer() {
    let peer = PeerId::random();
    let mut trust = Trust::default();
    assert!(trust.trusted(&peer), "no delay: an unseen peer is trusted");
    trust.add(peer, -1);
    assert!(
        !trust.trusted(&peer),
        "a failed request outweighs no answer"
    );
    trust.add(peer, 1);
    trust.delay = Duration::from_secs(3600);
    assert!(!trust.trusted(&peer), "never seen");
    trust.seen.insert(peer, Instant::now());
    assert!(!trust.trusted(&peer), "seen just now");
    trust
        .seen
        .insert(peer, Instant::now() - Duration::from_secs(3600));
    assert!(trust.trusted(&peer));
    trust.forget(&peer);
    assert!(!trust.trusted(&peer), "forgotten");
    assert!(trust.seen.is_empty() && trust.score.is_empty());
    trust
        .seen
        .insert(peer, Instant::now() - Duration::from_secs(3600));
    trust.retain(|_| false);
    assert!(!trust.trusted(&peer), "swept");
}

#[test]
fn anchors_keep_the_latest_distinct_peers() {
    let peers: Vec<PeerId> = (0..5).map(|_| PeerId::random()).collect();
    let at = |i: usize| -> Multiaddr { format!("/ip4/10.0.0.{i}/tcp/1").parse().unwrap() };
    let mut anchors = Vec::new();
    for (i, peer) in peers.iter().enumerate() {
        remember_anchor(&mut anchors, *peer, &at(i));
    }
    remember_anchor(&mut anchors, peers[2], &at(9));
    let expected: Vec<Multiaddr> = [(9, 2), (4, 4), (3, 3)]
        .iter()
        .map(|&(i, p)| at(i).with(Protocol::P2p(peers[p])))
        .collect();
    assert_eq!(anchors, expected);
}

#[test]
fn ip_groups_mask_the_prefix() {
    let limits = Limits::default();
    let group = |address: &str| ip_group(&address.parse().unwrap(), &limits);
    assert_eq!(group("/ip4/203.0.113.77/tcp/1"), "203.0.113.0/24");
    assert_eq!(
        group("/ip6/2001:db8:1:2::5/udp/1/quic-v1"),
        "2001:db8:1::/48"
    );
    let all = Limits {
        ipv4_prefix: 0,
        ipv6_prefix: 128,
        ..Limits::default()
    };
    assert_eq!(
        ip_group(&"/ip4/1.2.3.4".parse().unwrap(), &all),
        "0.0.0.0/0"
    );
    let ip6 = "/ip6/2001:db8::5".parse().unwrap();
    assert_eq!(ip_group(&ip6, &all), "2001:db8::5/128");
    assert_eq!(group("/memory/5"), "/memory/5");
}
