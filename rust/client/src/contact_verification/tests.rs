mod maintenance;

use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::{key_hash, Identity};
use dyapp_messaging::mls::Device;
use dyapp_profile::{Profile, ProfileCondition};

fn owner(secret: u8) -> Identity {
    Identity::from_secret(&[secret; 32])
}

fn store(secret: u8) -> Store {
    Store::open(&temp_db_path(), &owner(secret)).unwrap()
}

fn profile(secret: u8, version: u64, devices: &[u8]) -> VerifiedProfile {
    let identity = owner(secret);
    let mut certificates: Vec<_> = devices
        .iter()
        .map(|secret| {
            let key = owner(*secret).public_key();
            let device = Device::new(&identity, key, 1).unwrap();
            (key_hash(&key), device.certificate().clone())
        })
        .collect();
    certificates.sort_by_key(|(key, _)| *key);
    let profile = Profile {
        version,
        devices: certificates
            .into_iter()
            .map(|(_, certificate)| certificate)
            .collect(),
        ..Default::default()
    };
    dyapp_profile::verify(&profile.sign(&identity)).unwrap()
}

#[test]
fn generation_tracks_invalidation_without_changing_on_profile_admission() {
    let mut context = Context::default();
    let first = context.generation();
    let retained = first.clone();
    assert!(context.is_current(&first));
    assert!(!Context::default().is_current(&first));
    context.accept(profile(2, 1, &[3])).unwrap();
    assert!(context.is_current(&retained));

    context.clear();
    let second = context.generation();
    assert!(!context.is_current(&first));
    assert!(!context.is_current(&retained));
    assert!(context.is_current(&second));
    context.accept(profile(2, 1, &[3])).unwrap();
    assert!(!context.is_current(&first));

    context.clear();
    assert!(!context.is_current(&first));
    assert!(!context.is_current(&retained));
    assert!(!context.is_current(&second));
    assert!(context.is_current(&context.generation()));
}

#[test]
fn current_comparison_marks_the_held_profile() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    assert_eq!(context.state(&store, &peer).unwrap(), State::default());
    let ticket = context.begin(&store, &peer).unwrap();
    let accepted = context
        .confirm(&store, ticket, profile(2, 1, &[3]))
        .unwrap();
    assert_eq!(accepted.peer_id, peer);
    assert_eq!(
        context.state(&store, &peer).unwrap(),
        State {
            verified: true,
            new_device: false
        }
    );
}

#[test]
fn invalid_own_and_wrong_targets_never_mark_a_peer() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    assert!(matches!(
        context.begin(&store, &owner(1).peer_id()),
        Err(Error::OwnPeer)
    ));
    assert!(matches!(
        context.begin(&store, "not a peer"),
        Err(Error::InvalidPeer)
    ));
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(matches!(
        context.confirm(&store, ticket, profile(3, 1, &[])),
        Err(Error::WrongPeer)
    ));
    assert_eq!(context.profiles().all(), <&[VerifiedProfile]>::default());
    assert_eq!(context.state(&store, &peer).unwrap(), State::default());
}

#[test]
fn switching_away_and_back_does_not_restore_a_ticket() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    context.clear();
    context.clear();
    assert!(matches!(
        context.confirm(&store, ticket, profile(2, 1, &[])),
        Err(Error::ContextChanged)
    ));
    assert_eq!(context.profiles().all(), <&[VerifiedProfile]>::default());
    let fresh = context.begin(&store, &peer).unwrap();
    context.confirm(&store, fresh, profile(2, 1, &[])).unwrap();
    assert!(context.state(&store, &peer).unwrap().verified);
}

#[test]
fn full_unsigned_versions_keep_their_durable_order_after_reopen() {
    let path = temp_db_path();
    let peer = owner(2).peer_id();
    {
        let store = Store::open(&path, &owner(1)).unwrap();
        let mut context = Context::default();
        let ticket = context.begin(&store, &peer).unwrap();
        context
            .confirm(&store, ticket, profile(2, u64::MAX, &[3, 4]))
            .unwrap();
    }
    let store = Store::open_existing(&path, &owner(1)).unwrap();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(context
        .confirm(&store, ticket, profile(2, u64::MAX - 1, &[3]))
        .is_err());
}

#[test]
fn another_context_or_local_owner_cannot_use_a_ticket() {
    let first = store(1);
    let second = store(4);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&first, &peer).unwrap();
    assert!(matches!(
        context.confirm(&second, ticket, profile(2, 1, &[])),
        Err(Error::ContextChanged)
    ));
    let ticket = context.begin(&first, &peer).unwrap();
    let mut different = Context::default();
    assert!(matches!(
        different.confirm(&first, ticket, profile(2, 1, &[])),
        Err(Error::ContextChanged)
    ));
    assert_eq!(context.state(&first, &peer).unwrap(), State::default());
    assert_eq!(different.state(&second, &peer).unwrap(), State::default());
}

#[test]
fn rollback_and_unpublished_records_do_not_write_a_mark() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    context.accept(profile(2, 3, &[3])).unwrap();
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(matches!(
        context.confirm(&store, ticket, profile(2, 2, &[3])),
        Err(Error::StaleProfile)
    ));
    assert!(!context.state(&store, &peer).unwrap().verified);
    context.clear();
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(dyapp_profile::verify(&Profile::default().sign(&owner(2))).is_err());
    // Version zero cannot arrive from verification; retain the defensive in-process guard.
    let mut unpublished = profile(2, 1, &[]);
    unpublished.profile.version = 0;
    assert!(matches!(
        context.confirm(&store, ticket, unpublished),
        Err(Error::Unpublished)
    ));
    assert!(!context.state(&store, &peer).unwrap().verified);
}

#[test]
fn deletion_and_search_only_authority_cannot_receive_a_mark() {
    let store = store(1);
    let identity = owner(2);
    let peer = identity.peer_id();
    let source = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&identity);
    let deletion = Profile {
        previous: Some(ProfileCondition::observed(Some(&source)).unwrap()),
        ..Profile::tombstone(2, 0, 0)
    };
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    let deleted = dyapp_profile::verify(&deletion.sign(&identity)).unwrap();
    assert!(matches!(
        context.confirm(&store, ticket, deleted),
        Err(Error::Unpublished)
    ));
    assert!(!context.state(&store, &peer).unwrap().verified);
    context.clear();
    let ticket = context.begin(&store, &peer).unwrap();
    let withdrawal =
        dyapp_profile::verify_search(&Profile::search_tombstone(3, 0, 0).sign(&identity)).unwrap();
    assert!(matches!(
        context.confirm(&store, ticket, withdrawal),
        Err(Error::StaleProfile)
    ));
    assert_eq!(context.profiles().all(), <&[VerifiedProfile]>::default());
}

#[test]
fn a_fresh_context_cannot_lower_the_durable_observed_version() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    context
        .confirm(&store, ticket, profile(2, 3, &[3, 4]))
        .unwrap();
    context.clear();
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(context
        .confirm(&store, ticket, profile(2, 2, &[3]))
        .is_err());
}

#[test]
fn equal_version_confirmation_uses_the_already_held_authority() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let held = profile(2, 3, &[3]);
    let mut context = Context::default();
    context.accept(held.clone()).unwrap();
    let ticket = context.begin(&store, &peer).unwrap();
    let accepted = context
        .confirm(&store, ticket, profile(2, 3, &[4]))
        .unwrap();
    assert_eq!(accepted.profile.devices, held.profile.devices);
    assert!(context.state(&store, &peer).unwrap().verified);
}

#[test]
fn store_only_actions_validate_targets_before_mutation() {
    let store = store(1);
    let own = profile(1, 1, &[9]);
    store.set_verified(&own).unwrap();
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    context
        .confirm(&store, ticket, profile(2, 1, &[3]))
        .unwrap();
    context.accept(profile(2, 2, &[3, 4])).unwrap();
    let before = context.state(&store, &peer).unwrap();
    assert!(before.new_device);
    for action in [unverify, acknowledge_devices] {
        assert!(matches!(action(&store, &own.peer_id), Err(Error::OwnPeer)));
        for invalid in ["bad".to_owned(), peer.to_uppercase()] {
            assert!(matches!(action(&store, &invalid), Err(Error::InvalidPeer)));
        }
        assert_eq!(context.state(&store, &peer).unwrap(), before);
        assert!(store.verified(&own).unwrap());
    }
}

#[test]
fn store_only_actions_preserve_device_memory_and_independent_flags() {
    let store = store(1);
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    context
        .confirm(&store, ticket, profile(2, 1, &[3]))
        .unwrap();
    acknowledge_devices(&store, &peer).unwrap();
    assert!(context.state(&store, &peer).unwrap().verified);
    context.accept(profile(2, 2, &[3, 4])).unwrap();
    assert!(context.state(&store, &peer).unwrap().new_device);
    unverify(&store, &peer).unwrap();
    assert!(context.state(&store, &peer).unwrap().new_device);
    acknowledge_devices(&store, &peer).unwrap();
    assert_eq!(context.state(&store, &peer).unwrap(), State::default());
    context.accept(profile(2, 3, &[3])).unwrap();
    assert_eq!(context.state(&store, &peer).unwrap(), State::default());
    context.accept(profile(2, 4, &[3, 4])).unwrap();
    assert!(context.state(&store, &peer).unwrap().new_device);
}

#[test]
fn session_delegation_preserves_device_notice_and_unverify_semantics() {
    let identity = owner(1);
    let session = Session::open_or_create(store(1), &identity, &owner(9), &[8; 32]).unwrap();
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(session.store(), &peer).unwrap();
    session
        .confirm_contact(&mut context, ticket, profile(2, 1, &[3]))
        .unwrap();
    context.accept(profile(2, 2, &[3, 4])).unwrap();
    assert_eq!(
        session.contact_verification(&context, &peer).unwrap(),
        State {
            verified: false,
            new_device: true
        }
    );
    session.acknowledge_contact_devices(&peer).unwrap();
    assert_eq!(
        session.contact_verification(&context, &peer).unwrap(),
        State::default()
    );
    assert_eq!(
        session.contact_verification(&context, &peer).unwrap(),
        State::default()
    );
    let ticket = context.begin(session.store(), &peer).unwrap();
    session
        .confirm_contact(&mut context, ticket, profile(2, 2, &[3, 4]))
        .unwrap();
    session.unverify_contact(&peer).unwrap();
    assert_eq!(
        session.contact_verification(&context, &peer).unwrap(),
        State::default()
    );
    context.accept(profile(2, 3, &[3])).unwrap();
    assert_eq!(
        session.contact_verification(&context, &peer).unwrap(),
        State::default()
    );
    context.accept(profile(2, 4, &[3, 4])).unwrap();
    assert!(
        session
            .contact_verification(&context, &peer)
            .unwrap()
            .new_device
    );
}

#[test]
fn a_failed_write_keeps_the_previously_committed_mark() {
    let path = temp_db_path();
    let store = Store::open(&path, &owner(1)).unwrap();
    let peer = owner(2).peer_id();
    let mut context = Context::default();
    let ticket = context.begin(&store, &peer).unwrap();
    context
        .confirm(&store, ticket, profile(2, 1, &[3]))
        .unwrap();
    let database = rusqlite::Connection::open(path).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER refuse_mark BEFORE UPDATE ON `peers`
         BEGIN SELECT RAISE(ABORT, 'controlled storage failure'); END;",
        )
        .unwrap();
    let ticket = context.begin(&store, &peer).unwrap();
    assert!(matches!(
        context.confirm(&store, ticket, profile(2, 2, &[3, 4])),
        Err(Error::Store(_))
    ));
    let persisted: (bool, i64) = database
        .query_row(
            "SELECT `verified`, `seen_version` FROM `peers` WHERE `peer_id` = ?1",
            [peer],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(persisted, (true, 1));
}

#[test]
fn exact_unsigned_floors_survive_linking_and_keep_device_notices() {
    let boundary = u64::try_from(i64::MAX).unwrap();
    for version in [boundary, boundary + 1, boundary + 2, u64::MAX] {
        let source = store(1);
        source.set_verified(&profile(2, version, &[3])).unwrap();
        let mut linked = store(1);
        linked
            .import_link_state(&source.link_state().unwrap())
            .unwrap();
        assert!(linked.verified(&profile(2, version, &[3])).unwrap());
        assert!(linked.set_verified(&profile(2, version - 1, &[3])).is_err());
        assert!(linked.verified(&profile(2, version - 1, &[3, 4])).unwrap());
        assert!(!linked.new_device(&owner(2).peer_id()).unwrap());
        assert!(!linked.verified(&profile(2, version, &[3, 4])).unwrap());
        assert!(linked.new_device(&owner(2).peer_id()).unwrap());
        linked.acknowledge_new_device(&owner(2).peer_id()).unwrap();
        assert!(!linked.verified(&profile(2, version, &[3])).unwrap());
        assert!(!linked.new_device(&owner(2).peer_id()).unwrap());
        linked.set_verified(&profile(2, version, &[3, 4])).unwrap();
        assert!(linked.verified(&profile(2, version, &[3, 4])).unwrap());
    }
}

#[test]
fn a_legacy_clamped_floor_stays_unverified_until_explicit_maximum_confirmation() {
    let path = temp_db_path();
    let source = Store::open(&path, &owner(1)).unwrap();
    let peer = owner(2).peer_id();
    source.set_verified(&profile(2, u64::MAX, &[3])).unwrap();
    let database = rusqlite::Connection::open(&path).unwrap();
    database
        .execute(
            "UPDATE `peers` SET `seen_version_exact` = NULL WHERE `peer_id` = ?1",
            [&peer],
        )
        .unwrap();
    let mut linked = store(1);
    linked
        .import_link_state(&source.link_state().unwrap())
        .unwrap();
    for store in [&source, &linked] {
        assert!(!store.verified(&profile(2, u64::MAX - 1, &[3, 4])).unwrap());
        assert!(store.set_verified(&profile(2, u64::MAX - 1, &[3])).is_err());
        assert!(!store.new_device(&peer).unwrap());
        assert!(!store.verified(&profile(2, u64::MAX, &[3])).unwrap());
        store.set_verified(&profile(2, u64::MAX, &[3])).unwrap();
        assert!(store.verified(&profile(2, u64::MAX, &[3])).unwrap());
    }
}

#[test]
fn an_older_link_snapshot_cannot_erase_a_locally_observed_unsigned_floor() {
    let source = store(1);
    source
        .set_verified(&profile(2, u64::MAX - 1, &[3]))
        .unwrap();
    source.set_blocked("unrelated preference", true).unwrap();
    let mut target = store(1);
    target.set_verified(&profile(2, u64::MAX, &[3, 4])).unwrap();
    assert!(target
        .import_link_state(&source.link_state().unwrap())
        .is_err());
    assert!(!target.blocked("unrelated preference").unwrap());
    assert!(target
        .set_verified(&profile(2, u64::MAX - 1, &[3]))
        .is_err());
    assert!(target.verified(&profile(2, u64::MAX, &[3, 4])).unwrap());
}

#[test]
fn exact_metadata_mismatch_refuses_contact_writes_and_snapshot_export() {
    let path = temp_db_path();
    let store = Store::open(&path, &owner(1)).unwrap();
    let peer = owner(2).peer_id();
    store.set_verified(&profile(2, 3, &[3])).unwrap();
    let database = rusqlite::Connection::open(&path).unwrap();
    database
        .execute(
            "UPDATE `peers` SET `seen_version_exact` = ?2 WHERE `peer_id` = ?1",
            rusqlite::params![peer, 2_u64.to_be_bytes().as_slice()],
        )
        .unwrap();
    assert!(store.verified(&profile(2, 4, &[3, 4])).is_err());
    assert!(store.set_verified(&profile(2, 4, &[3, 4])).is_err());
    assert!(store.link_state().is_err());
    let verified: bool = database
        .query_row(
            "SELECT `verified` FROM `peers` WHERE `peer_id` = ?1",
            [&peer],
            |row| row.get(0),
        )
        .unwrap();
    assert!(verified);
}

#[test]
fn writer_admission_precedes_device_observation_and_confirmation() {
    let path = temp_db_path();
    let store = Store::open(&path, &owner(1)).unwrap();
    let peer = owner(2).peer_id();
    store.set_verified(&profile(2, 2, &[3])).unwrap();
    let database = rusqlite::Connection::open(&path).unwrap();
    let transaction =
        rusqlite::Transaction::new_unchecked(&database, rusqlite::TransactionBehavior::Immediate)
            .unwrap();
    database.execute(
        "UPDATE `peers` SET `seen_version` = ?2, `seen_version_exact` = ?3 WHERE `peer_id` = ?1",
        rusqlite::params![peer, i64::MAX, u64::MAX.to_be_bytes().as_slice()],
    ).unwrap();
    // An explicit competing writer is the readiness signal, with no timing assumption.
    // Read-only stale fast paths must also fail to acquire authority while it is held.
    assert!(store.verified(&profile(2, 1, &[3])).is_err());
    assert!(store.set_verified(&profile(2, 3, &[3])).is_err());
    transaction.commit().unwrap();
    assert!(store.set_verified(&profile(2, 3, &[3])).is_err());
    assert!(store.verified(&profile(2, u64::MAX, &[3])).unwrap());
}
