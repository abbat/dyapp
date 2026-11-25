use super::*;

fn address(port: u16) -> Multiaddr {
    format!("/ip4/127.0.0.1/tcp/{port}").parse().unwrap()
}

#[test]
fn endpoints_survive_reopen_but_network_scopes_do_not_mix() {
    let dir = dyapp_test_support::test_dir().unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let peer = PeerId::random();
    let cache = Cache::at(dir.path(), "public");
    cache.record(peer, address(7070));
    drop(cache);
    let reopened = Cache::at(dir.path(), "public");
    let entries = reopened.candidates();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].peer, peer);
    assert_eq!(
        entries[0].address.to_string(),
        format!("/ip4/127.0.0.1/tcp/7070/p2p/{peer}")
    );
    assert_eq!(Cache::at(dir.path(), "private").candidates(), []);
    let text = fs::read_to_string(&reopened.inner.lock().unwrap().path).unwrap();
    assert!(!text.contains("score"));
    assert!(!text.contains("latency"));
}

#[test]
fn duplicate_responses_do_not_rewrite_or_grow_the_cache() {
    let dir = dyapp_test_support::test_dir().unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let cache = Cache::at(dir.path(), "");
    let peer = PeerId::random();
    cache.record(peer, address(1));
    let path = cache.inner.lock().unwrap().path.clone();
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    cache.record(peer, address(1));
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(cache.candidates().len(), 1);
}

#[test]
fn bounds_evict_old_group_and_peer_entries_before_growth() {
    let dir = dyapp_test_support::test_dir().unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let cache = Cache::at(dir.path(), "");
    for port in 1..=3 {
        cache.record(PeerId::random(), address(port));
    }
    let first = PeerId::random();
    cache.record(first, address(10));
    cache.record(first, address(11));
    cache.record(first, address(12));
    let entries = cache.candidates();
    assert_eq!(
        entries.iter().filter(|entry| entry.peer == first).count(),
        2
    );
    assert!(entries.len() <= MAX_GROUP_ENTRIES);
    for port in 20..=28 {
        cache.record(PeerId::random(), address(port));
    }
    assert!(cache.candidates().len() <= MAX_GROUP_ENTRIES);
}

#[test]
fn corrupt_or_foreign_files_fail_closed_without_touching_seeds() {
    let dir = dyapp_test_support::test_dir().unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let cache = Cache::at(dir.path(), "");
    let path = cache.inner.lock().unwrap().path.clone();
    fs::write(&path, b"dyapp-node-cache 1\nname private\n").unwrap();
    drop(cache);
    assert_eq!(Cache::at(dir.path(), "").candidates(), []);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "dyapp-node-cache 1\nname private\n"
    );
}

#[test]
fn new_endpoints_coalesce_disk_writes_for_one_minute() {
    let dir = dyapp_test_support::test_dir().unwrap();
    dyapp_p2p_net::set_id_pow_bits(0);
    let cache = Cache::at(dir.path(), "");
    let started = Instant::now();
    cache.record_at(PeerId::random(), address(1), started);
    let path = cache.inner.lock().unwrap().path.clone();
    let saved = || fs::read_to_string(&path).unwrap();
    assert_eq!(saved().matches("node ").count(), 1);
    for (port, seconds) in [(2, 1), (3, 59)] {
        cache.record_at(
            PeerId::random(),
            address(port),
            started + Duration::from_secs(seconds),
        );
    }
    assert_eq!(cache.candidates().len(), 3);
    assert_eq!(saved().matches("node ").count(), 1);
    cache.record_at(
        PeerId::random(),
        address(4),
        started + Duration::from_secs(60),
    );
    assert_eq!(cache.candidates().len(), 4);
    assert_eq!(saved().matches("node ").count(), 4);
}
