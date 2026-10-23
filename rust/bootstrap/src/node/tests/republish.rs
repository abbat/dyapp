//! Holder-side republish tests.
use super::*;
use crate::node::republish::REPUBLISH_WARMUP;
use crate::republish::Store;

fn node() -> (Service, Swarm<Behaviour>) {
    let dir = format!("/tmp/ai/test-republish-{}", uuid::Uuid::new_v4());
    let mut config = crate::NodeConfig::default();
    config.storage.dir = dir.clone().into();
    config.network.id_pow_bits = 0;
    config.search.min_profile_pow_bits = 0;
    config.limits.bytes_per_second = 10_000;
    let mut service = Service::new(crate::BootstrapStore::new(&dir).unwrap(), config);
    service.media = Some(crate::media::MediaStore::open(&service.config.storage.dir).unwrap());
    let mut swarm = swarm(
        Keypair::generate_ed25519(),
        &service.config.limits,
        &service.config.roles,
    )
    .unwrap();
    add_peer(&mut swarm);
    (service, swarm)
}

fn add_peer(swarm: &mut Swarm<Behaviour>) {
    swarm
        .behaviour_mut()
        .kad
        .add_address(&PeerId::random(), "/ip4/127.0.0.1/tcp/1".parse().unwrap());
}

fn hold_mailbox(service: &Service, mailbox: u8) {
    let record = dyapp_identity::Identity::generate().sign(
        dyapp_identity::Domain::Envelope,
        proto::Envelope {
            id: vec![mailbox; 16],
            mailbox: vec![mailbox; 32],
            ciphertext: vec![3],
            ..Default::default()
        }
        .encode_to_vec(),
    );
    assert_eq!(
        service.store_mailbox(&record, "local", &[]).unwrap().status,
        i32::from(Status::Ok)
    );
}

/// The time the warmup of a bootstrap finished now has passed.
fn ready(replicas: &mut Replicas) -> Instant {
    let now = Instant::now();
    replicas.republish.bootstrapped(now);
    now + REPUBLISH_WARMUP
}

/// Settles every lookup of the running handovers.
fn finish_lookups(replicas: &mut Replicas, ok: bool) {
    let queries: Vec<_> = replicas.republish.queries.keys().copied().collect();
    for query in queries {
        replicas.republish.query_done(query, ok);
    }
}

#[tokio::test]
async fn waits_for_bootstrap_warmup() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let mut replicas = Replicas::default();
    let now = Instant::now();
    replicas.scan(&mut swarm, &service, now);
    assert!(
        replicas.republish.scanner.pop(Store::Mailbox).is_none(),
        "scanned unbootstrapped"
    );
    replicas.republish.bootstrapped(now);
    replicas.republish.bootstrapped(now + REPUBLISH_WARMUP);
    replicas.scan(
        &mut swarm,
        &service,
        now + REPUBLISH_WARMUP - Duration::from_nanos(1),
    );
    assert!(
        replicas.republish.scanner.pop(Store::Mailbox).is_none(),
        "scanned in the warmup"
    );
    replicas.scan(&mut swarm, &service, now + REPUBLISH_WARMUP);
    assert_eq!(
        replicas.republish.scanner.pop(Store::Mailbox).unwrap().0,
        [1; 32]
    );
}

#[tokio::test]
async fn hands_every_store_to_the_repair_paths() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let owner = dyapp_identity::Identity::generate();
    let profile = dyapp_profile::Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    assert_eq!(
        service.store_profile(&profile).unwrap().status,
        i32::from(Status::Ok)
    );
    let keep = proto::MediaKeep {
        version: 1,
        hashes: vec![dyapp_identity::sha256(b"blob").to_vec()],
        time: chrono::Utc::now().timestamp().unsigned_abs(),
    };
    let record = owner.sign(dyapp_identity::Domain::MediaKeep, keep.encode_to_vec());
    let media = service.media.as_ref().unwrap();
    media.keep_signed("owner", &keep, &record).unwrap();
    let mut replicas = Replicas::default();
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 3);
    let count = |pick: fn(&Lookup) -> bool| replicas.lookups.values().filter(|l| pick(l)).count();
    assert_eq!(
        count(|lookup| matches!(lookup, Lookup::Repair(key) if key == &[1; 32])),
        usize::from(REPLICAS)
    );
    assert_eq!(
        count(|lookup| matches!(lookup, Lookup::ProfileRepair(_))),
        usize::from(REPLICAS)
    );
    assert_eq!(replicas.media_repairs.len(), 1);
}

#[tokio::test]
async fn unchanged_routing_republishes_nothing_until_a_new_closest_node() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let mut replicas = Replicas::default();
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    replicas.republish(&mut swarm, &service, now);
    finish_lookups(&mut replicas, true);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 0);
    replicas.scan(&mut swarm, &service, now);
    assert!(replicas.republish.scanner.pop(Store::Mailbox).is_none());
    add_peer(&mut swarm);
    replicas.scan(&mut swarm, &service, now);
    assert!(replicas.republish.scanner.pop(Store::Mailbox).is_some());
}

#[tokio::test]
async fn failed_handover_stays_dirty() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let mut replicas = Replicas::default();
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    replicas.republish(&mut swarm, &service, now);
    let query = *replicas.republish.queries.keys().next().unwrap();
    replicas.republish.query_done(query, false);
    finish_lookups(&mut replicas, true);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 0);
    replicas.scan(&mut swarm, &service, now);
    assert!(replicas.republish.scanner.pop(Store::Mailbox).is_some());
}

#[tokio::test]
async fn owner_repair_blocks_republish() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let mut replicas = Replicas::default();
    replicas.repaired.insert(vec![1; 32]);
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 0);
    assert!(replicas.lookups.is_empty());
    replicas.scan(&mut swarm, &service, now);
    assert!(replicas.republish.scanner.pop(Store::Mailbox).is_some());
}

#[tokio::test]
async fn keeps_sixteen_in_flight() {
    let (service, mut swarm) = node();
    for mailbox in 0..20 {
        hold_mailbox(&service, mailbox);
    }
    let mut replicas = Replicas::default();
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 16);
    assert_eq!(replicas.lookups.len(), 16 * usize::from(REPLICAS));
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 16);
    finish_lookups(&mut replicas, true);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 4);
}

#[tokio::test]
async fn busy_node_does_not_republish() {
    let (service, mut swarm) = node();
    hold_mailbox(&service, 1);
    let mut replicas = Replicas::default();
    let now = ready(&mut replicas);
    replicas.scan(&mut swarm, &service, now);
    service.traffic.add(5_000);
    replicas.republish(&mut swarm, &service, now);
    assert_eq!(replicas.republish.len(), 0);
    assert!(replicas.lookups.is_empty());
    assert!(replicas.republish.scanner.pop(Store::Mailbox).is_some());
}
