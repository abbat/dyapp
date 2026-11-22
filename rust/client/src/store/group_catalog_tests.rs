//! Catalog persistence, admission and rollback fixtures.
use super::{tests::person, Error, Store};
use crate::{prepare_put, PreparedEnvelope};
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;
use dyapp_profile::VerifiedProfile;

struct GroupFixture {
    identity: Identity,
    alice: Device,
    bob: Device,
    bob_identity: Identity,
    alice_profile: VerifiedProfile,
    bob_profile: VerifiedProfile,
    group: dyapp_messaging::mls::Group,
    write: PreparedEnvelope,
}

fn group_fixture() -> GroupFixture {
    let (identity, alice, alice_profile) = person();
    let (bob_identity, bob, bob_profile) = person();
    let (group, welcome) = alice
        .create_group(
            &[bob.key_package(false).unwrap()],
            &[alice_profile.clone(), bob_profile.clone()],
        )
        .unwrap();
    GroupFixture {
        identity,
        alice,
        bob,
        bob_identity,
        alice_profile,
        bob_profile,
        group,
        write: prepare_put(&[7; 32], welcome).unwrap(),
    }
}

#[test]
fn failed_group_staging_rolls_back_catalog_and_outbox() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let f = group_fixture();
    let mut store = Store::open(&test_dir.path().join("client-1.db"), &f.identity).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE INSERT ON `mls_state`
         BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
    let sealed = f.alice.seal(&[9; 32]).unwrap();
    assert!(matches!(
        store.stage_group(&f.group, &f.bob_profile, &sealed, &[f.write]),
        Err(Error::Sql(_))
    ));
    assert_eq!(
        store.known_groups().unwrap(),
        Vec::<(Vec<u8>, String)>::new()
    );
    assert!(store.due_outbound(0).unwrap().is_empty());
}

#[test]
fn group_catalog_and_outbox_reopen_with_the_sealed_device() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let f = group_fixture();
    let path = test_dir.path().join("client-1.db");
    let mut store = Store::open(&path, &f.identity).unwrap();
    let sealed = f.alice.seal(&[9; 32]).unwrap();
    store
        .stage_group(
            &f.group,
            &f.bob_profile,
            &sealed,
            std::slice::from_ref(&f.write),
        )
        .unwrap();
    drop(store);
    let store = Store::open(&path, &f.identity).unwrap();
    assert_eq!(
        store.known_groups().unwrap(),
        [(f.group.id().to_vec(), f.bob_profile.peer_id.clone())]
    );
    assert_eq!(
        store.due_outbound(0).unwrap()[0].as_bytes(),
        f.write.as_bytes()
    );
    let opened = Device::open(&store.sealed_device().unwrap().unwrap(), &[9; 32]).unwrap();
    let loaded = store.load_groups(&opened).unwrap();
    assert_eq!(loaded[0].0.id(), f.group.id());
    assert_eq!(loaded[0].1, f.bob_profile.peer_id);
}

#[test]
fn established_partner_binding_reopens_after_its_last_device_is_revoked() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let mut f = group_fixture();
    let path = test_dir.path().join("client-1.db");
    let mut store = Store::open(&path, &f.identity).unwrap();
    let sealed = f.alice.seal(&[9; 32]).unwrap();
    store
        .stage_group(&f.group, &f.bob_profile, &sealed, &[])
        .unwrap();
    let revoked = dyapp_profile::Profile {
        version: 2,
        ..Default::default()
    };
    let revoked = dyapp_profile::verify(&revoked.sign(&f.bob_identity)).unwrap();
    crate::chat::remove(&mut f.group, &f.alice, &[f.alice_profile.clone(), revoked]).unwrap();
    assert_eq!(f.group.members().unwrap(), vec![f.alice_profile.peer_id]);
    let sealed = f.alice.seal(&[9; 32]).unwrap();
    store.stage_outbound(&sealed, &[]).unwrap();
    drop(store);
    let store = Store::open(&path, &f.identity).unwrap();
    let opened = Device::open(&store.sealed_device().unwrap().unwrap(), &[9; 32]).unwrap();
    let loaded = store.load_groups(&opened).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0.id(), f.group.id());
    assert_eq!(loaded[0].1, f.bob_profile.peer_id);
    assert_eq!(loaded[0].0.members().unwrap(), vec![f.identity.peer_id()]);
    // This restoration exception must not register a new owner-only group for another peer.
    let mut fresh = Store::open(&test_dir.path().join("client-2.db"), &f.identity).unwrap();
    assert!(matches!(
        fresh.stage_group(&f.group, &f.bob_profile, &sealed, &[]),
        Err(Error::GroupConflict)
    ));
}

#[test]
fn group_catalog_rejects_wrong_peers_and_missing_device_groups() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let f = group_fixture();
    let (_, _, carol_profile) = person();
    let mut store = Store::open(&test_dir.path().join("client-1.db"), &f.identity).unwrap();
    let sealed = f.alice.seal(&[9; 32]).unwrap();
    for wrong in [&carol_profile, &f.alice_profile] {
        assert!(matches!(
            store.stage_group(&f.group, wrong, &sealed, &[]),
            Err(Error::GroupConflict)
        ));
        assert_eq!(
            store.known_groups().unwrap(),
            Vec::<(Vec<u8>, String)>::new()
        );
    }
    store
        .stage_group(&f.group, &f.bob_profile, &sealed, &[])
        .unwrap();
    assert!(matches!(
        store.load_groups(&f.bob),
        Err(Error::GroupMismatch)
    ));
    store
        .db
        .execute(
            "UPDATE `conversation_groups` SET `peer_id` = ?1 WHERE `group_id` = ?2",
            rusqlite::params![carol_profile.peer_id, f.group.id()],
        )
        .unwrap();
    let opened = Device::open(&store.sealed_device().unwrap().unwrap(), &[9; 32]).unwrap();
    assert!(matches!(
        store.load_groups(&opened),
        Err(Error::GroupMismatch)
    ));
}
