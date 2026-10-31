use super::{prepare, Error, Session};
use crate::chat::{self, Body, Event};
use crate::store::messages::MessageKey;
use crate::store::{InboxKey, InboxKind, Store};
use dyapp_identity::{Domain, Identity};
use dyapp_messaging::hlc::Timestamp;
use dyapp_messaging::mls::Device;
use dyapp_p2p_net::proto::{self, EnvelopeClass};
use dyapp_profile::Profile;
use prost::Message as _;

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
    // A match opens the chat, so the second text is not a second pending request.
    let alice_peer = alice_id.peer_id();
    let like = store
        .interact(
            &bob_profile.peer_id,
            crate::interactions::Kind::Like,
            &[1],
            1_000,
        )
        .unwrap()
        .unwrap();
    store
        .apply_interaction(&bob_profile.peer_id, &alice_peer, &[2], like)
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
    let (offline_id, offline_writes) = session
        .stage_text(&group_id, b"offline", &seal_key)
        .unwrap();
    assert_ne!(offline_id, [1; 16]);
    assert_eq!(offline_writes.len(), 1);
    let own = MessageKey {
        group: group_id.clone(),
        author: alice_id.peer_id(),
        id: offline_id,
    };
    assert_eq!(
        session
            .store
            .chat_message(&own)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"offline"[..])
    );
    let due = session.due_outbound(0).unwrap();
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == writes[0].as_bytes()));
    assert!(due
        .iter()
        .any(|write| write.as_bytes() == offline_writes[0].as_bytes()));
    drop(session);
    let mut session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
    assert_eq!(
        session.chat_message(&own).unwrap().unwrap().text.as_deref(),
        Some(&b"offline"[..])
    );
    let before_invalid = session.store.sealed_device().unwrap();
    assert!(matches!(
        session.stage_edit(&[9], offline_id, b"wrong group", &seal_key),
        Err(Error::UnknownMessage)
    ));
    assert!(matches!(
        session.stage_delete(&group_id, [9; 16], &seal_key),
        Err(Error::UnknownMessage)
    ));
    assert_eq!(session.store.sealed_device().unwrap(), before_invalid);
    let (edit_id, edit_writes) = session
        .stage_edit(&group_id, offline_id, b"revised", &seal_key)
        .unwrap();
    assert_ne!(edit_id, [0; 16]);
    assert_eq!(
        session.chat_message(&own).unwrap().unwrap().text.as_deref(),
        Some(&b"revised"[..])
    );
    let (delete_id, delete_writes) = session
        .stage_delete(&group_id, offline_id, &seal_key)
        .unwrap();
    assert_ne!(delete_id, edit_id);
    assert!(matches!(
        session.stage_edit(&group_id, offline_id, b"restore", &seal_key),
        Err(Error::UnknownMessage)
    ));
    let current = session.chat_message(&own).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    drop(session);
    let mut session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
    assert!(session.chat_message(&own).unwrap().unwrap().deleted);
    let due = session.due_outbound(0).unwrap();
    for exact in [
        &writes[0],
        &offline_writes[0],
        &edit_writes[0],
        &delete_writes[0],
    ] {
        assert!(due.iter().any(|write| write.as_bytes() == exact.as_bytes()));
    }
    assert_eq!(session.clear_chat(&group_id).unwrap(), 2);
    assert!(session.chat_message(&own).unwrap().is_none());
    assert!(session.due_outbound(0).unwrap().is_empty());
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
    assert_eq!(
        payload,
        dyapp_identity::sha256(&Body::text_with_id([1; 16], b"hi".to_vec()).encode())
    );
    let message = MessageKey {
        group: group.id().to_vec(),
        author: alice_id.peer_id(),
        id: [1; 16],
    };
    assert_eq!(
        session
            .store
            .chat_message(&message)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"hi"[..])
    );
    drop(session);

    // Restarted before the ACK: the joined group and both pending ACKs are back.
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert!(session.store.has_inbox(&key(2)).unwrap());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
    assert_eq!(
        session
            .store
            .chat_message(&message)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"hi"[..])
    );
    let again = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([2; 16], b"again".to_vec()),
    )
    .unwrap();
    // A second text before Bob replies is a second pending request: acked, never shown.
    assert!(session
        .stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert!(session.store.has_inbox(&key(3)).unwrap());
    let dropped = MessageKey {
        id: [2; 16],
        ..message.clone()
    };
    assert!(session.store.chat_message(&dropped).unwrap().is_none());

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
        alice_id,
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
    let message = MessageKey {
        group: group.id().to_vec(),
        author: alice_id.peer_id(),
        id: [1; 16],
    };
    assert!(session.store.chat_message(&message).unwrap().is_none());
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
    assert_eq!(
        session
            .store
            .chat_message(&message)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"hi"[..])
    );
}

#[test]
fn received_edit_and_delete_keep_only_current_projection() {
    let path = temp_path();
    let seal_key = [8; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let message = MessageKey {
        group: group.id().to_vec(),
        author: alice_id.peer_id(),
        id: [4; 16],
    };
    let bodies = [
        Body::text_with_id(message.id, b"original private text".to_vec()),
        Body::Edit {
            target: message.id,
            operation: [5; 16],
            hlc: dyapp_messaging::hlc::Timestamp::new(100, 0).into(),
            text: b"replacement private text".to_vec(),
        },
        Body::Delete {
            target: message.id,
            operation: [6; 16],
            hlc: dyapp_messaging::hlc::Timestamp::new(200, 0).into(),
        },
    ];
    for (i, body) in bodies.iter().enumerate() {
        let sent = chat::message(&mut group, &alice, body).unwrap();
        assert!(matches!(
            session.stage_envelope(&key(i as u8 + 2), &sent[0].mls, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
        let payload: Vec<u8> = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
                [key(i as u8 + 2).id.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(payload, dyapp_identity::sha256(&body.encode()));
        let current = session.store.chat_message(&message).unwrap().unwrap();
        match i {
            0 => assert_eq!(current.text.as_deref(), Some(&b"original private text"[..])),
            1 => assert_eq!(
                current.text.as_deref(),
                Some(&b"replacement private text"[..])
            ),
            _ => assert!(current.deleted && current.text.is_none()),
        }
    }
    drop(session);
    let session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    let current = session.store.chat_message(&message).unwrap().unwrap();
    assert!(current.edited && current.deleted && current.text.is_none());
    assert_eq!(
        session.acks([9; 32], vec![]).unwrap(),
        [key(1), key(2), key(3), key(4)]
    );
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

#[test]
fn presence_goes_to_a_match_hourly_and_is_seen_only_from_a_match() {
    let seal_key = [6; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let (a, b) = (alice_id.peer_id(), bob_id.peer_id());
    let now = crate::now() * 1000;
    let presence = |group: &mut dyapp_messaging::mls::Group, wall_ms| {
        let hlc = dyapp_messaging::hlc::Timestamp::new(wall_ms, 0).into();
        chat::message(group, &alice, &Body::Presence { hlc }).unwrap()[0]
            .mls
            .clone()
    };

    // Not a match yet: nothing is sent and a received presence is dropped.
    assert!(session.stage_presence(now, &seal_key).unwrap().is_empty());
    let early = presence(&mut group, now);
    assert!(session
        .stage_envelope(&key(2), &early, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert!(!session.store.active_recently(&a, now).unwrap());

    for (sender, target, id) in [(&a, &b, 1), (&b, &a, 2)] {
        let event = crate::interactions::Interaction {
            kind: crate::interactions::Kind::Like,
            event_id: [id; 16],
            hlc: 1,
        };
        session
            .store
            .apply_interaction(sender, target, b"d", event)
            .unwrap();
    }
    let writes = session.stage_presence(now, &seal_key).unwrap();
    assert_eq!(writes.len(), 1);
    assert!(session
        .stage_presence(now + 1, &seal_key)
        .unwrap()
        .is_empty());
    let hour = now + 3_600_000;
    assert_eq!(session.stage_presence(hour, &seal_key).unwrap().len(), 1);
    // Unsent presence is given up when the receiver would stop showing it.
    let due = |session: &Session| session.due_outbound(i64::MAX).unwrap().len();
    assert_eq!(due(&session), 2);
    let expiry = |at: u64| i64::try_from(at + 2 * 3_600_000).unwrap();
    assert_eq!(session.store.expire_outbound(expiry(now) - 1).unwrap(), 0);
    assert_eq!(session.store.expire_outbound(expiry(now)).unwrap(), 1);
    assert_eq!(session.store.expire_outbound(expiry(hour)).unwrap(), 1);
    assert_eq!(due(&session), 0);
    session.store.set_send_presence(false).unwrap();
    let later = now + 2 * 3_600_000;
    assert!(session.stage_presence(later, &seal_key).unwrap().is_empty());

    // A match's presence shows even with ours hidden; one from the future is refused.
    let seen = presence(&mut group, now);
    assert!(matches!(
        session.stage_envelope(&key(3), &seen, &profiles, &seal_key),
        Ok(Some(Event::Message { .. }))
    ));
    assert!(session.store.active_recently(&a, now).unwrap());
    let future = presence(&mut group, now + 3_600_000);
    assert!(matches!(
        session.stage_envelope(&key(4), &future, &profiles, &seal_key),
        Ok(Some(Event::Refused(_)))
    ));
}

#[test]
fn read_receipts_are_opt_in_and_marked_only_by_the_unblocked_partner() {
    let seal_key = [5; 32];
    let Invite {
        mut session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let (a, b) = (alice_id.peer_id(), bob_id.peer_id());
    let id = group.id().to_vec();
    let text = Body::text_with_id([1; 16], b"hi".to_vec());
    let sent = chat::message(&mut group, &alice, &text);
    session
        .stage_envelope(&key(2), &sent.unwrap()[0].mls, &profiles, &seal_key)
        .unwrap();
    session.stage_text(&id, b"yes", &seal_key).unwrap();

    // Off by default; once on, one receipt per newly seen text.
    let receipts = |s: &mut Session| s.stage_read(&id, [1; 16], &seal_key).unwrap().len();
    assert_eq!(receipts(&mut session), 0);
    session.store.set_send_read_receipts(true).unwrap();
    assert_eq!(receipts(&mut session), 1);
    assert_eq!(receipts(&mut session), 0);

    // Alice's receipt marks Bob's own texts read, up to the one it names.
    for own in [2, 3] {
        let text = MessageKey {
            group: id.clone(),
            author: b.clone(),
            id: [own; 16],
        };
        session.store.record_chat_text(&text, b"own").unwrap();
    }
    let read = |group: &mut dyapp_messaging::mls::Group, up_to| {
        chat::message(group, &alice, &Body::Read { up_to }).unwrap()[0]
            .mls
            .clone()
    };
    let first = read(&mut group, [2; 16]);
    assert!(matches!(
        session.stage_envelope(&key(3), &first, &profiles, &seal_key),
        Ok(Some(Event::Message { .. }))
    ));
    assert_eq!(session.store.read_up_to(&id, &a).unwrap(), Some([2; 16]));
    session.store.set_blocked(&a, true).unwrap();
    let blocked = read(&mut group, [3; 16]);
    session
        .stage_envelope(&key(4), &blocked, &profiles, &seal_key)
        .unwrap();
    assert_eq!(session.store.read_up_to(&id, &a).unwrap(), Some([2; 16]));
    assert_eq!(session.acks([9; 32], vec![]).unwrap().len(), 4);
}

#[test]
fn a_strangers_other_bodies_are_dropped_until_a_reply() {
    let seal_key = [4; 32];
    let Invite {
        mut session,
        alice,
        mut group,
        welcome,
        profiles,
        ..
    } = bob_invited(&temp_path(), &seal_key);
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let id = group.id().to_vec();
    let mut send = |body: &Body| {
        chat::message(&mut group, &alice, body).unwrap()[0]
            .mls
            .clone()
    };
    let hi = send(&Body::text_with_id([1; 16], b"hi".to_vec()));
    let ask = Body::TokenRequest(chat::TokenRequest {
        asks: vec![chat::TokenAsk {
            mailbox: vec![1; 32],
            epoch: 1,
            token_key: vec![2; 32],
        }],
    });
    let forward = Body::Forwarded(Box::new(Body::text_with_id([3; 16], b"fw".to_vec())));
    let early = [send(&ask), send(&forward)];
    let late = [send(&ask), send(&forward)];
    assert!(session
        .stage_envelope(&key(2), &hi, &profiles, &seal_key)
        .unwrap()
        .is_some());
    // Before Bob replies: acked, and no event, so no token answer and no history.
    for (n, bytes) in (3..).zip(&early) {
        assert!(session
            .stage_envelope(&key(n), bytes, &profiles, &seal_key)
            .unwrap()
            .is_none());
        assert!(session.store.has_inbox(&key(n)).unwrap());
    }
    session.stage_text(&id, b"yes", &seal_key).unwrap();
    for (n, bytes) in (5..).zip(&late) {
        assert!(matches!(
            session.stage_envelope(&key(n), bytes, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
    }
}

#[test]
fn a_first_text_invites_and_the_pair_then_sends_to_its_lowest_group() {
    let seal_key = [5; 32];
    let path = temp_path();
    let Invite {
        session,
        bob_id,
        alice_id,
        alice,
        mut group,
        welcome,
        profiles,
    } = bob_invited(&path, &seal_key);
    let mut session = session;
    let alice_peer = alice_id.peer_id();
    let alice_key: [u8; 32] = alice.device_key().unwrap().try_into().unwrap();
    let packages = [(
        crate::mailbox(&alice_key).to_vec(),
        alice.key_package(false).unwrap(),
    )];
    let (_, writes) = session
        .stage_text_to(&profiles, &packages, b"hi", &seal_key)
        .unwrap();
    // The Welcome and the text, both to Alice's device, in one commit.
    assert_eq!(writes.len(), 2);
    assert!(writes
        .iter()
        .all(|w| w.mailbox() == &crate::mailbox(&alice_key)));
    let own = session.pair_group(&alice_peer).unwrap();

    // A restart retries the same staged bytes and reopens the new group.
    drop(session);
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    let due: Vec<_> = session.due_outbound(i64::MAX).unwrap();
    let bytes = |writes: &[crate::PreparedEnvelope]| {
        let mut all: Vec<_> = writes.iter().map(|w| w.as_bytes().to_vec()).collect();
        all.sort();
        all
    };
    assert_eq!(bytes(&due), bytes(&writes));
    assert_eq!(session.pair_group(&alice_peer), Some(own.clone()));

    // Alice made a group at the same time and replies in it: both groups are kept.
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let reply = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([7; 16], b"yo".to_vec()),
    )
    .unwrap()[0]
        .mls
        .clone();
    assert!(session
        .stage_envelope(&key(2), &reply, &profiles, &seal_key)
        .unwrap()
        .is_some());
    let lowest = own.clone().min(group.id().to_vec());
    let (id, _) = session
        .stage_text_to(&profiles, &[], b"again", &seal_key)
        .unwrap();
    let sent = MessageKey {
        group: lowest,
        author: bob_id.peer_id(),
        id,
    };
    assert!(session.chat_message(&sent).unwrap().is_some());
    assert_eq!(session.store.known_groups().unwrap().len(), 2);
}

#[test]
fn a_first_like_invites_and_stages_the_like_with_the_welcome() {
    let seal_key = [5; 32];
    let path = temp_path();
    let Invite {
        session,
        bob_id,
        alice_id,
        alice,
        profiles,
        ..
    } = bob_invited(&path, &seal_key);
    let mut session = session;
    let alice_peer = alice_id.peer_id();
    let alice_key: [u8; 32] = alice.device_key().unwrap().try_into().unwrap();
    let packages = [(
        crate::mailbox(&alice_key).to_vec(),
        alice.key_package(false).unwrap(),
    )];
    let before = crate::now() * 1000;
    let (like, writes) = session
        .stage_first_like(&profiles, &packages, &seal_key)
        .unwrap()
        .unwrap();
    assert!(Timestamp::from(like.hlc).wall_ms() >= before);
    // The Welcome and the like, both to Alice's device.
    assert_eq!(writes.len(), 2);
    assert!(session.likes(&alice_peer).unwrap().ours);
    let group = session.pair_group(&alice_peer).unwrap();

    // A restart keeps the group, the like and both staged writes.
    drop(session);
    let session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert_eq!(session.pair_group(&alice_peer), Some(group));
    assert!(session.likes(&alice_peer).unwrap().ours);
    assert_eq!(session.due_outbound(i64::MAX).unwrap().len(), 2);
}

#[test]
fn a_write_carries_the_token_of_today() {
    let seal_key = [5; 32];
    let mailbox = [7; 32];
    let today = (crate::now() / 86_400) as u32;
    let mut store = Store::open(&temp_path(), &Identity::generate()).unwrap();
    let token = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: store
            .token_key(&mailbox, today, &seal_key)
            .unwrap()
            .to_vec(),
        epoch: today,
    };
    let signed = Identity::generate().sign(Domain::WriteToken, token.encode_to_vec());
    store
        .set_write_token(&mailbox, today, &signed.encode_to_vec())
        .unwrap();
    let delivery = chat::Delivery {
        mailbox: mailbox.to_vec(),
        mls: vec![1],
    };
    let writes = prepare(&store, vec![delivery], EnvelopeClass::Message, &seal_key).unwrap();
    assert_eq!(crate::tokens::epoch(&writes[0]), Some(today));
}

#[test]
fn the_first_open_creates_the_device_and_later_opens_keep_it() {
    let (identity, device_key) = (Identity::generate(), Identity::generate());
    let path = crate::store::tests::temp_db_path();
    let seal_key = [5; 32];
    let first = Session::open_or_create(
        Store::open(&path, &identity).unwrap(),
        &identity,
        &device_key,
        &seal_key,
    )
    .unwrap()
    .certificate()
    .unwrap();
    let again = Session::open_or_create(
        Store::open(&path, &identity).unwrap(),
        &identity,
        &Identity::generate(),
        &seal_key,
    )
    .unwrap();
    assert_eq!(again.certificate().unwrap(), first);
    let listed = dyapp_profile::DeviceCertificate::decode(first.payload.as_slice()).unwrap();
    assert_eq!(listed.device_key, device_key.public_key().to_vec());
}
