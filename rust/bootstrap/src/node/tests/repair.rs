//! Profile repair schedule and shared byte-budget tests.
use super::*;

fn repair_after_interval(
    replicas: &mut Replicas,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    record: &SignedRecord,
    key: &[u8],
    now: Instant,
) {
    let edge = now + PROFILE_REPAIR_INTERVAL;
    let early = edge.checked_sub(Duration::from_nanos(1)).unwrap();
    replicas.repair_profile(swarm, service, record.clone(), early);
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    replicas.lookups.clear();
    replicas.repair_profile(swarm, service, record.clone(), edge);
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    assert_eq!(replicas.profile_repaired[key].0, edge);
}

fn check_gap_statuses(
    replicas: &mut Replicas,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    remote: PeerId,
    record: &SignedRecord,
) {
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
            matches!(
                replicas.profile_gap(swarm, service, remote, record, &response),
                Ok(Some(_))
            ),
            sends
        );
    }
}

fn check_repair_budget(
    replicas: &mut Replicas,
    swarm: &mut Swarm<Behaviour>,
    service: &Service,
    remote: PeerId,
    record: SignedRecord,
) {
    service.traffic.add(10_000);
    let response = proto::ProfileResponse {
        status: Status::NotFound.into(),
        ..Default::default()
    };
    assert!(replicas
        .profile_gap(swarm, service, remote, &record, &response)
        .is_err());
    assert!(replicas
        .profile_inventory(swarm, service, PeerId::random(), Arc::new(record))
        .is_err());
    assert_eq!(
        replicas.profile_inventories.len(),
        1,
        "repair exceeded its budget"
    );
}

#[tokio::test]
async fn profile_repair_only_fills_gaps_once_an_hour_within_budget() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir
        .path()
        .join("case-2290")
        .to_string_lossy()
        .into_owned();
    let mut config = crate::config::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.limits.bytes_per_second = 10_000;
    let manual = crate::clock::Manual::at(1_800_000_000);
    let clock = manual.clock();
    let service = Service::with_clock(
        crate::storage::BootstrapStore::new(&dir).unwrap(),
        config,
        clock.clone(),
    )
    .unwrap();
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
    let owner = dyapp_identity::Identity::try_generate().unwrap();
    let record = dyapp_profile::Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    let key = dyapp_identity::key_hash(&owner.public_key()).to_vec();
    let mut replicas = Replicas::default();
    let now = clock.instant();
    replicas.repair_profile(&mut swarm, &service, record.clone(), now);
    assert_eq!(replicas.lookups.len(), usize::from(REPLICAS));
    repair_after_interval(&mut replicas, &mut swarm, &service, &record, &key, now);
    for _ in 0..REPLICAS {
        let _ = replicas.profile_inventory(&mut swarm, &service, remote, Arc::new(record.clone()));
    }
    assert_eq!(
        replicas.profile_inventories.len(),
        1,
        "co-located replicas are inventoried once"
    );
    check_gap_statuses(&mut replicas, &mut swarm, &service, remote, &record);
    check_repair_budget(&mut replicas, &mut swarm, &service, remote, record);
}

#[tokio::test]
async fn background_media_repair_holds_no_channel() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let dir = test_dir.path().to_string_lossy().into_owned();
    let mut config = crate::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    let mut service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config).unwrap();
    service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    let owner = dyapp_identity::Identity::try_generate().unwrap();
    let hash = dyapp_identity::sha256(b"unavailable blob").to_vec();
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![hash.clone()],
        time: crate::clock::Clock::system().timestamp().unsigned_abs(),
    };
    let record = owner.sign(dyapp_identity::Domain::MediaKeep, keep.encode_to_vec());
    let mut replicas = Replicas::default();
    let now = Instant::now();
    let id = replicas
        .start_media_repair(&mut swarm, &service, record, now)
        .expect("background repair starts");
    assert!(replicas.media_repairs[&id].1.is_none());
    assert_eq!(
        replicas.finish_media_repairs(&mut swarm, &service, now + crate::media_repair::TIMEOUT),
        vec![(id, false)],
    );
    assert!(!replicas.media_repairs.contains_key(&id));
    assert_eq!(replicas.media_repaired.result(&hash, now), Some(true));
}
