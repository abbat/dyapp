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
    let identity = Identity::try_generate().unwrap();
    let device = Device::new(&identity, Identity::try_generate().unwrap().public_key(), 1).unwrap();
    let profile = Profile {
        version: 1,
        devices: vec![device.certificate().clone()],
        ..Default::default()
    }
    .sign(&identity);
    (identity, device, dyapp_profile::verify(&profile).unwrap())
}

#[path = "tests/outbound.rs"]
mod outbound;

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
pub(super) struct Invite {
    pub(super) session: Session,
    pub(super) bob_id: Identity,
    pub(super) alice_id: Identity,
    pub(super) alice: Device,
    pub(super) group: dyapp_messaging::mls::Group,
    pub(super) welcome: Vec<u8>,
    pub(super) profiles: [dyapp_profile::VerifiedProfile; 2],
}

pub(super) fn bob_invited(path: &std::path::Path, seal_key: &[u8; 32]) -> Invite {
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

fn attachment_photo() -> Body {
    Body::Attachment(crate::chat::AttachmentMessage {
        id: [3; 16],
        hlc: Timestamp::new(1_000, 0).into(),
        attachment: crate::attachment::Attachment {
            key: [1; 32],
            plaintext_sha256: [2; 32],
            blob_sha256: [3; 32],
            release_secret: [4; 32],
            size: 12,
            preview: vec![5],
            mime: "image/jpeg".into(),
            width: 20,
            height: 10,
        },
    })
}

fn assert_forwarded_payload(path: &std::path::Path, forward: &Body) {
    let payload: Vec<u8> = rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
            [key(4).id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, dyapp_identity::sha256(&forward.encode()));
}

fn assert_forwarded_photo_after_restart(
    path: &std::path::Path,
    bob_id: &Identity,
    alice_id: &Identity,
    seal_key: &[u8; 32],
) {
    let session = Session::open(Store::open(path, bob_id).unwrap(), seal_key).unwrap();
    let history = session
        .store
        .current_chat_attachments(&alice_id.peer_id())
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].key.id, [3; 16]);
    assert!(history[0].forwarded && history[0].unread);
    assert!(session.store.has_inbox(&key(4)).unwrap());
}

#[test]
fn attachment_request_is_dropped_then_forwarded_photo_survives_restart() {
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
    session
        .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
        .unwrap();
    let photo = attachment_photo();
    let before = chat::message(&mut group, &alice, &photo).unwrap();
    assert!(session
        .stage_envelope(&key(2), &before[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_none());
    assert!(session
        .store
        .current_chat_attachments(&alice_id.peer_id())
        .unwrap()
        .is_empty());

    let request = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hello".to_vec()),
    )
    .unwrap();
    assert!(session
        .stage_envelope(&key(3), &request[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_some());
    session.stage_text(group.id(), b"reply", &seal_key).unwrap();
    let forward = Body::Forwarded(Box::new(photo));
    let after = chat::message(&mut group, &alice, &forward).unwrap();
    assert!(session
        .stage_envelope(&key(4), &after[0].mls, &profiles, &seal_key)
        .unwrap()
        .is_some());
    assert_forwarded_payload(&path, &forward);
    drop(session);

    assert_forwarded_photo_after_restart(&path, &bob_id, &alice_id, &seal_key);
}

fn receive_first_text(
    path: &std::path::Path,
    session: &mut Session,
    group: &mut dyapp_messaging::mls::Group,
    alice: &Device,
    alice_id: &Identity,
    profiles: &[dyapp_profile::VerifiedProfile; 2],
    seal_key: &[u8; 32],
) -> MessageKey {
    let hi = chat::message(group, alice, &Body::text_with_id([1; 16], b"hi".to_vec())).unwrap();
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, profiles, seal_key),
        Ok(Some(Event::Message { .. }))
    ));
    let payload: Vec<u8> = rusqlite::Connection::open(path)
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
    message
}

fn assert_restart_and_drop_second_text(
    session: &mut Session,
    group: &mut dyapp_messaging::mls::Group,
    alice: &Device,
    profiles: &[dyapp_profile::VerifiedProfile; 2],
    seal_key: &[u8; 32],
    message: &MessageKey,
) {
    assert!(session.store.has_inbox(&key(2)).unwrap());
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
    assert_eq!(
        session
            .store
            .chat_message(message)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"hi"[..])
    );
    let again = chat::message(
        group,
        alice,
        &Body::text_with_id([2; 16], b"again".to_vec()),
    )
    .unwrap();
    // A second text before Bob replies is a second pending request: acked, never shown.
    assert!(session
        .stage_envelope(&key(3), &again[0].mls, profiles, seal_key)
        .unwrap()
        .is_none());
    assert!(session.store.has_inbox(&key(3)).unwrap());
    let dropped = MessageKey {
        id: [2; 16],
        ..message.clone()
    };
    assert!(session.store.chat_message(&dropped).unwrap().is_none());
}

fn assert_duplicate_ack_only_for_same_mailbox(session: &mut Session) {
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
    assert_eq!(
        session
            .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
            .unwrap(),
        Some(Event::Joined(group.id().to_vec()))
    );
    assert_eq!(
        session.store.known_groups().unwrap(),
        [(group.id().to_vec(), alice_id.peer_id())]
    );
    let message = receive_first_text(
        &path,
        &mut session,
        &mut group,
        &alice,
        &alice_id,
        &profiles,
        &seal_key,
    );
    drop(session);

    // Restarted before the ACK: the joined group and both pending ACKs are back.
    let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
    assert_restart_and_drop_second_text(
        &mut session,
        &mut group,
        &alice,
        &profiles,
        &seal_key,
        &message,
    );
    assert_duplicate_ack_only_for_same_mailbox(&mut session);
}

#[tokio::test]
async fn blocked_peer_is_refused_before_outbound_interaction_uses_network() {
    let owner = Identity::try_generate().unwrap();
    let device_key = Identity::try_generate().unwrap();
    let peer = person().2;
    let seal_key = [7; 32];
    let store = Store::open(&temp_path(), &owner).unwrap();
    let mut session = Session::open_or_create(store, &owner, &device_key, &seal_key).unwrap();
    session.store.set_blocked(&peer.peer_id, true).unwrap();
    let own = Profile {
        version: 1,
        devices: vec![session.certificate().unwrap()],
        ..Default::default()
    }
    .sign(&owner);
    let own = dyapp_profile::verify(&own).unwrap();
    assert!(matches!(
        session
            .interact_with(
                &mut [],
                &[own, peer],
                crate::interactions::Kind::Like,
                &seal_key,
            )
            .await,
        Err(Error::Store(crate::store::Error::BlockedPeer))
    ));
    assert!(session.store.due_outbound(0).unwrap().is_empty());
}

#[tokio::test]
async fn block_with_no_issued_tokens_is_local_even_without_nodes() {
    let owner = Identity::try_generate().unwrap();
    let device_key = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let seal_key = [7; 32];
    let store = Store::open(&temp_path(), &owner).unwrap();
    let mut session = Session::open_or_create(store, &owner, &device_key, &seal_key).unwrap();
    session.block(&mut [], &device_key, &peer).await.unwrap();
    assert!(session.store.blocked(&peer).unwrap());
}

fn blocked_owner_is_refused(seal_key: &[u8; 32]) {
    let invite = bob_invited(&temp_path(), seal_key);
    let mut session = invite.session;
    session
        .store
        .set_blocked(&invite.alice_id.peer_id(), true)
        .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(1), &invite.welcome, &invite.profiles, seal_key),
        Ok(Some(Event::Refused(_)))
    ));
    assert_eq!(session.store.known_groups().unwrap().len(), 0);
}

#[test]
fn a_blocked_owner_is_not_joined_and_its_messages_are_acked_unseen() {
    let seal_key = [7; 32];
    blocked_owner_is_refused(&seal_key);

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
fn a_message_from_an_owner_not_yet_fetched_waits_and_opens_with_the_profile() {
    let seal_key = [7; 32];
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
    let hi = chat::message(
        &mut group,
        &alice,
        &Body::text_with_id([1; 16], b"hi".to_vec()),
    )
    .unwrap();
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, &[], &seal_key),
        Ok(Some(Event::UnknownOwner(_)))
    ));
    assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
    assert!(matches!(
        session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
        Ok(Some(Event::Message { .. }))
    ));
}

/// Why a pass goes back to the head as soon as a profile resolves: past the ratchet's small
/// out-of-order window an earlier text can no longer be opened.
#[test]
fn a_text_opened_after_many_later_ones_is_refused() {
    let seal_key = [7; 32];
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
    let texts: Vec<_> = (0..25u8)
        .map(|i| {
            let body = Body::text_with_id([i + 1; 16], vec![b'a' + i]);
            chat::message(&mut group, &alice, &body).unwrap().remove(0)
        })
        .collect();
    assert!(matches!(
        session.stage_envelope(&key(2), &texts[0].mls, &[], &seal_key),
        Ok(Some(Event::UnknownOwner(_)))
    ));
    // Later texts of an unanswered request may go unshown; they still advance the ratchet.
    for (i, text) in (0u8..).zip(&texts).skip(20) {
        session
            .stage_envelope(&key(i + 2), &text.mls, &profiles, &seal_key)
            .unwrap();
    }
    assert!(matches!(
        session.stage_envelope(&key(2), &texts[0].mls, &profiles, &seal_key),
        Ok(Some(Event::Refused(_)))
    ));
}

fn message_body(event: Result<Option<Event>, Error>) -> Body {
    let Some(Event::Message { body, .. }) = event.unwrap() else {
        panic!("expected the message after recovery");
    };
    body
}

fn assert_chat_text(store: &Store, key: &MessageKey, expected: &[u8]) {
    assert_eq!(
        store.chat_message(key).unwrap().unwrap().text.as_deref(),
        Some(expected)
    );
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
    let body = message_body(session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key));
    assert!(body == Body::text_with_id([1; 16], b"hi".to_vec()));
    assert_chat_text(&session.store, &message, b"hi");
}

fn assert_operation_payload(path: &std::path::Path, i: u8, body: &Body) {
    let payload: Vec<u8> = rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
            [key(i + 2).id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(payload, dyapp_identity::sha256(&body.encode()));
}

fn assert_current_operation(store: &Store, message: &MessageKey, i: u8) {
    let current = store.chat_message(message).unwrap().unwrap();
    match i {
        0 => assert_eq!(current.text.as_deref(), Some(&b"original private text"[..])),
        1 => assert_eq!(
            current.text.as_deref(),
            Some(&b"replacement private text"[..])
        ),
        _ => assert!(current.deleted && current.text.is_none()),
    }
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
    for (i, body) in (0u8..).zip(&bodies) {
        let sent = chat::message(&mut group, &alice, body).unwrap();
        assert!(matches!(
            session.stage_envelope(&key(i + 2), &sent[0].mls, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
        assert_operation_payload(&path, i, body);
        assert_current_operation(&session.store, &message, i);
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

fn assert_hourly_match_presence(
    session: &mut Session,
    a: &str,
    b: &str,
    now: u64,
    seal_key: &[u8; 32],
) {
    for (sender, target, id) in [(a, b, 1), (b, a, 2)] {
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
    let writes = session.stage_presence(now, seal_key).unwrap();
    assert_eq!(writes.len(), 1);
    assert!(session
        .stage_presence(now + 1, seal_key)
        .unwrap()
        .is_empty());
    let hour = now + 3_600_000;
    assert_eq!(session.stage_presence(hour, seal_key).unwrap().len(), 1);
    // Unsent presence is given up when the receiver would stop showing it.
    let due = |session: &Session| session.due_outbound(i64::MAX).unwrap().len();
    assert_eq!(due(session), 2);
    let expiry = |at: u64| i64::try_from(at + 2 * 3_600_000).unwrap();
    assert_eq!(session.store.expire_outbound(expiry(now) - 1).unwrap(), 0);
    assert_eq!(session.store.expire_outbound(expiry(now)).unwrap(), 1);
    assert_eq!(session.store.expire_outbound(expiry(hour)).unwrap(), 1);
    assert_eq!(due(session), 0);
    session.store.set_send_presence(false).unwrap();
    let later = now + 2 * 3_600_000;
    assert!(session.stage_presence(later, seal_key).unwrap().is_empty());
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

    assert_hourly_match_presence(&mut session, &a, &b, now, &seal_key);

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
    let stage =
        |s: &mut Session, n, bytes: &[u8]| s.stage_envelope(&key(n), bytes, &profiles, &seal_key);
    stage(&mut session, 1, &welcome).unwrap();
    let (a, b) = (alice_id.peer_id(), bob_id.peer_id());
    let id = group.id().to_vec();
    let text = Body::text_with_id([1; 16], b"hi".to_vec());
    let sent = chat::message(&mut group, &alice, &text);
    stage(&mut session, 2, &sent.unwrap()[0].mls).unwrap();
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
        stage(&mut session, 3, &first),
        Ok(Some(Event::Message { .. }))
    ));
    assert_eq!(session.store.read_up_to(&id, &a).unwrap(), Some([2; 16]));
    session.store.set_blocked(&a, true).unwrap();
    let blocked = read(&mut group, [3; 16]);
    stage(&mut session, 4, &blocked).unwrap();
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

mod invites;

#[test]
fn a_write_carries_the_token_of_today() {
    let seal_key = [5; 32];
    let mailbox = [7; 32];
    let today = u32::try_from(crate::now() / 86_400).unwrap();
    let mut store = Store::open(&temp_path(), &Identity::try_generate().unwrap()).unwrap();
    let token = proto::WriteToken {
        mailbox: mailbox.to_vec(),
        token_key: store
            .token_key(&mailbox, today, &seal_key)
            .unwrap()
            .to_vec(),
        epoch: today,
    };
    let signed = Identity::try_generate()
        .unwrap()
        .sign(Domain::WriteToken, token.encode_to_vec());
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
    let (identity, device_key) = (
        Identity::try_generate().unwrap(),
        Identity::try_generate().unwrap(),
    );
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
        &Identity::try_generate().unwrap(),
        &seal_key,
    )
    .unwrap();
    assert_eq!(again.certificate().unwrap(), first);
    let listed = dyapp_profile::DeviceCertificate::decode(first.payload.as_slice()).unwrap();
    assert_eq!(listed.device_key, device_key.public_key().to_vec());
}

#[test]
fn reopened_sessions_can_share_identity_network_selection_state() {
    let identity = Identity::try_generate().unwrap();
    let device = Identity::try_generate().unwrap();
    let path = crate::store::tests::temp_db_path();
    let scores = std::sync::Arc::default();
    let first = Session::open_or_create_with_scores(
        Store::open(&path, &identity).unwrap(),
        &identity,
        &device,
        &[5; 32],
        std::sync::Arc::clone(&scores),
    )
    .unwrap();
    let reopened = Session::open_with_scores(
        Store::open(&path, &identity).unwrap(),
        &[5; 32],
        std::sync::Arc::clone(&scores),
    )
    .unwrap();
    assert!(std::sync::Arc::ptr_eq(&first.scores(), &scores));
    assert!(std::sync::Arc::ptr_eq(&reopened.scores(), &scores));
}
