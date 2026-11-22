use super::*;

#[test]
fn refresh_turn_is_bounded_ordered_and_excludes_the_owner() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let store = Store::open(&path, &owner(1)).unwrap();
    assert_eq!(refresh_target(&store, 0).unwrap(), None);
    let own = owner(1).peer_id();
    let peers = [owner(2).peer_id(), owner(3).peer_id(), owner(4).peer_id()];
    let database = rusqlite::Connection::open(&path).unwrap();
    // Fixture catalog order deliberately differs from insertion order; no network/MLS traffic.
    for (group, peer) in [(3_u8, &peers[2]), (0, &own), (2, &peers[1]), (1, &peers[0])] {
        database
            .execute(
                "INSERT INTO `conversation_groups` (`group_id`, `peer_id`) VALUES (?1, ?2)",
                rusqlite::params![[group].as_slice(), peer],
            )
            .unwrap();
    }
    let before = store.known_groups().unwrap();
    for (now, expected) in [(0, 0), (59, 0), (60, 1), (119, 1), (120, 2), (180, 0)] {
        assert_eq!(
            refresh_target(&store, now).unwrap(),
            Some(peers[expected].clone())
        );
    }
    assert_eq!(
        refresh_target(&store, u64::MAX - 60).unwrap(),
        Some(peers[2].clone())
    );
    assert_eq!(store.known_groups().unwrap(), before);
    // Preserve the established per-conversation weighting instead of silently deduplicating peers.
    database
        .execute(
            "INSERT INTO `conversation_groups` (`group_id`, `peer_id`) VALUES (?1, ?2)",
            rusqlite::params![[4_u8].as_slice(), &peers[0]],
        )
        .unwrap();
    assert_eq!(refresh_target(&store, 240).unwrap(), Some(peers[0].clone()));
}

#[test]
fn contact_readback_admits_only_the_requested_owner_and_current_generation() {
    let (_test_dir, store) = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let obsolete = context.generation();
    context.clear();
    assert!(!context
        .accept_contact_if_current(&obsolete, &store, &peer, profile(2, 3, &[3]))
        .unwrap());
    assert!(context.profiles().get(&peer).is_none());
    let generation = context.generation();
    assert!(context
        .accept_contact_if_current(&generation, &store, &peer, profile(2, 3, &[3]))
        .unwrap());
    assert!(matches!(
        context.accept_contact_if_current(&generation, &store, &peer, profile(3, 9, &[4])),
        Err(crate::Error::Input("unknownPeer"))
    ));
    assert!(matches!(
        context.accept_contact_if_current(
            &generation,
            &store,
            &owner(1).peer_id(),
            profile(1, 9, &[4])
        ),
        Err(crate::Error::Input("ownPeer"))
    ));
    assert_eq!(context.profiles().get(&peer).unwrap().profile.version, 3);
    assert_eq!(context.profiles().all().len(), 1);
}

#[test]
fn contact_readback_keeps_newer_and_equal_authority() {
    let (_test_dir, store) = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let generation = context.generation();
    let held = profile(2, 3, &[3]);
    context.accept(held.clone()).unwrap();
    assert!(matches!(
        context.accept_contact_if_current(&generation, &store, &peer, profile(2, 2, &[3])),
        Err(crate::Error::Rollback)
    ));
    let mut equal = held.profile.clone();
    equal.name = "Equal-version fork".into();
    let equal = dyapp_profile::verify(&equal.sign(&owner(2))).unwrap();
    assert!(context
        .accept_contact_if_current(&generation, &store, &peer, equal)
        .unwrap());
    assert_eq!(context.profiles().get(&peer), Some(&held));
    assert!(context.is_current(&generation));
}
