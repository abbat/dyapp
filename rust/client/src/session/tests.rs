use super::{Error, Session, StoredMessage};
use crate::chat::{self, Body, Event};
use crate::store::{InboxKey, InboxKind, Store};
use dyapp_identity::Identity;
use dyapp_messaging::mls::Device;
use dyapp_profile::Profile;

fn person() -> (Identity, Device, dyapp_profile::VerifiedProfile) {
    let identity = Identity::generate();
    let device = Device::new(&identity, Identity::generate().public_key(), 1).unwrap();
    let profile = Profile {
        version: 1,
        devices: vec![device.certificate().clone()],
        ..Default::default()
    }
    .sign(&identity);
    (identity, device, dyapp_profile::verify(&profile).unwrap())
}

#[test]
fn message_is_staged_before_network_and_reopens_exact_bytes() {
    let (alice_id, alice, alice_profile) = person();
    let (_, bob, bob_profile) = person();
    let (group, _) = alice
        .create_group(
            &[bob.key_package(false).unwrap()],
            &[alice_profile, bob_profile.clone()],
        )
        .unwrap();
    let group_id = group.id().to_vec();
    let seal_key = [3; 32];
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-send-session-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let mut store = Store::open(&path, &alice_id).unwrap();
    store
        .stage_group(&group, &bob_profile, &alice.seal(&seal_key).unwrap(), &[])
        .unwrap();
    let mut session = Session::open(store, &seal_key).unwrap();
    let writes = session
        .stage_message(
            &group_id,
            &Body::text_with_id([1; 16], b"hello".to_vec()),
            &seal_key,
        )
        .unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(
        session.due_outbound(0).unwrap()[0].as_bytes(),
        writes[0].as_bytes()
    );
    drop(session);
    let session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
    assert_eq!(
        session.due_outbound(0).unwrap()[0].as_bytes(),
        writes[0].as_bytes()
    );
}

fn temp_path() -> std::path::PathBuf {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    std::env::temp_dir().join(format!(
        "dyapp-receive-session-{:032x}.db",
        u128::from_be_bytes(random)
    ))
}

fn key(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [9; 32],
        id: [id; 16],
    }
}

/// Bob's session over a fresh store, invited by Alice to `group` with `welcome`.
struct Invite {
    session: Session,
    bob_id: Identity,
    alice_id: Identity,
    alice: Device,
    group: dyapp_messaging::mls::Group,
    welcome: Vec<u8>,
    profiles: [dyapp_profile::VerifiedProfile; 2],
}

fn bob_invited(path: &std::path::Path, seal_key: &[u8; 32]) -> Invite {
    let (alice_id, alice, alice_profile) = person();
    let (bob_id, bob, bob_profile) = person();
    let profiles = [alice_profile, bob_profile];
    let (group, welcome) = alice
        .create_group(&[bob.key_package(false).unwrap()], &profiles)
        .unwrap();
    let mut store = Store::open(path, &bob_id).unwrap();
    store
        .stage_outbound(&bob.seal(seal_key).unwrap(), &[])
        .unwrap();
    Invite {
        session: Session::open(store, seal_key).unwrap(),
        bob_id,
        alice_id,
        alice,
        group,
        welcome,
        profiles,
    }
}

#[test]
fn received_envelopes_commit_before_ack_and_survive_restart() {
    let path = temp_path();
    let seal_key = [5; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal_key);
    assert!(
        session
            .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
            .unwrap()
            == Some(Event::Joined(group.id().to_vec()))
    );
    assert_eq!(
        session.store.known_groups().unwrap(),
        [(group.id().to_vec(), alice_id.peer_id())]
    );
    let hi = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hi".to_vec()),
    )
    .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
        Ok(Some(Event::Message { .. }))
    ));
    let payload: Vec<u8> = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
            [[2_u8; 16].as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        <StoredMessage as prost::Message>::decode(payload.as_slice()).unwrap()
            == StoredMessage {
                group: group.id().to_vec(),
                sender: alice_id.peer_id(),
                body: Body::text_with_id([1; 16], b"hi".to_vec()).encode(),
            }
    );
    drop(session);

    // Restarted before the ACK: the joined group and both pending ACKs are back.
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert!(session.store.has_inbox(&key(2)).unwrap());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
    let again = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"again".to_vec()),
    )
    .unwrap();
    let Some(Event::Message { body, .. }) = session
        .stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key)
        .unwrap()
    else {
        panic!("expected the next message");
    };
    assert!(body == Body::text_with_id([1; 16], b"again".to_vec()));

    // A duplicate seen after its ACK is acknowledged again; another mailbox is not.
    for id in [1, 2, 3] {
        assert!(session.store.mark_acked(&key(id)).unwrap());
    }
    let other = InboxKey {
        mailbox: [8; 32],
        id: [4; 16],
    };
    session
        .store
        .stage_inbound(b"sealed", &other, InboxKind::StateChange, b"", None)
        .unwrap();
    assert_eq!(session.acks([9; 32], vec![key(2)]).unwrap(), [key(2)]);
}

#[test]
fn a_blocked_owner_is_not_joined_and_its_messages_are_acked_unseen() {
    let seal_key = [7; 32];
    let invite = bob_invited(&temp_path(), &seal_key);
    let mut session = invite.session;
    session
        .store
        .set_blocked(&invite.alice_id.peer_id(), true)
        .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(1), &invite.welcome, &invite.profiles, &seal_key),
        Ok(Some(Event::Refused(_)))
    ));
    assert!(session.store.known_groups().unwrap().is_empty());

    let Invite {
        mut session,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
        ..
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    session
        .store
        .set_blocked(&alice_id.peer_id(), true)
        .unwrap();
    let hi = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hi".to_vec()),
    )
    .unwrap();
    assert!(session
        .stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
    // The ratchet moved on: after an unblock the next message still opens.
    session
        .store
        .set_blocked(&alice_id.peer_id(), false)
        .unwrap();
    let again = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"again".to_vec()),
    )
    .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key),
        Ok(Some(Event::Message { .. }))
    ));
}

#[test]
fn failed_receive_commit_neither_acks_nor_loses_the_envelope() {
    let path = temp_path();
    let seal_key = [6; 32];
    let Invite {
        mut session,
        alice,
        mut group,
        welcome,
        profiles,
        ..
    } = bob_invited(&path, &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let hi = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hi".to_vec()),
    )
    .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
         BEGIN SELECT RAISE(ABORT, 'test failure'); END",
    )
    .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
        Err(Error::Store(_))
    ));
    assert!(!session.store.has_inbox(&key(2)).unwrap());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
        Err(Error::Invalidated)
    ));

    db.execute_batch("DROP TRIGGER `fail_mls`").unwrap();
    session.recover(&seal_key).unwrap();
    let Some(Event::Message { body, .. }) = session
        .stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key)
        .unwrap()
    else {
        panic!("expected the message after recovery");
    };
    assert!(body == Body::text_with_id([1; 16], b"hi".to_vec()));
}

#[test]
fn refused_envelope_is_marked_without_changing_mls_state() {
    let path = temp_path();
    let seal_key = [7; 32];
    let Invite {
        mut session,
        profiles,
        ..
    } = bob_invited(&path, &seal_key);
    let sealed = session.store.sealed_device().unwrap();
    assert!(matches!(
        session.stage_envelope(&key(1), b"not mls", &profiles, &seal_key),
        Ok(Some(Event::Refused(_)))
    ));
    assert!(session.store.has_inbox(&key(1)).unwrap());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
    assert_eq!(session.store.sealed_device().unwrap(), sealed);
    assert!(session.device.is_some());
}

#[test]
fn failed_store_write_invalidates_session_until_recovery() {
    let (alice_id, alice, alice_profile) = person();
    let (_, bob, bob_profile) = person();
    let (group, _) = alice
        .create_group(
            &[bob.key_package(false).unwrap()],
            &[alice_profile, bob_profile.clone()],
        )
        .unwrap();
    let group_id = group.id().to_vec();
    let seal_key = [4; 32];
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-send-session-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let mut store = Store::open(&path, &alice_id).unwrap();
    let sealed = alice.seal(&seal_key).unwrap();
    store
        .stage_group(&group, &bob_profile, &sealed, &[])
        .unwrap();
    let mut session = Session::open(store, &seal_key).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
         BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
    assert!(matches!(
        session.stage_message(
            &group_id,
            &Body::text_with_id([1; 16], b"hello".to_vec()),
            &seal_key
        ),
        Err(Error::Store(_))
    ));
    assert!(matches!(
        session.stage_message(
            &group_id,
            &Body::text_with_id([1; 16], b"retry".to_vec()),
            &seal_key
        ),
        Err(Error::Invalidated)
    ));
    assert!(session.due_outbound(0).unwrap().is_empty());
    assert_eq!(session.store.sealed_device().unwrap(), Some(sealed));
    session.recover(&seal_key).unwrap();
    assert!(session.device.is_some());
}
