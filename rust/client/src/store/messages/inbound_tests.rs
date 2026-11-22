use super::{Body, Error, InboxKey, MessageKey, Store, VerifiedChatSender};
use crate::attachment::Attachment;
use crate::chat::AttachmentMessage;
use dyapp_identity::{sha256, Identity};
use dyapp_messaging::hlc::Timestamp;

fn photo(id: u8, time: u64) -> Body {
    Body::Attachment(AttachmentMessage {
        id: [id; 16],
        hlc: Timestamp::new(time, 0).into(),
        attachment: Attachment {
            key: [3; 32],
            plaintext_sha256: [4; 32],
            blob_sha256: [5; 32],
            release_secret: [6; 32],
            size: 12,
            preview: vec![7],
            mime: "image/jpeg".into(),
            width: 20,
            height: 10,
        },
    })
}

fn open_photo_conversation(
    store: &mut Store,
    sender: VerifiedChatSender,
    group: &[u8],
    owner: &Identity,
    device: &[u8; 32],
) {
    let request = Body::text_with_id([2; 16], b"hello".to_vec());
    assert!(store
        .stage_chat_inbound(b"mls-2", &inbox(2), sender, &request, 1_000)
        .unwrap());
    let reply = Body::text_with_id([3; 16], b"yes".to_vec());
    let owner_id = owner.peer_id();
    let own = VerifiedChatSender {
        group,
        author: &owner_id,
        device,
    };
    assert!(store
        .stage_chat_inbound(b"mls-3", &inbox(3), own, &reply, 1_000)
        .unwrap());
}

fn assert_future_photo_rejected(store: &mut Store, sender: VerifiedChatSender) {
    assert!(matches!(
        store.stage_chat_inbound(b"future", &inbox(4), sender, &photo(4, 61_001), 1_000),
        Err(Error::ClockSkew(_))
    ));
    assert!(!store.has_inbox(&inbox(4)).unwrap());
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"mls-3");
}

#[test]
fn inbound_photo_projects_once_with_snapshot_and_digest() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let owner = Identity::try_generate().unwrap();
    let peer = Identity::try_generate().unwrap().peer_id();
    let group = [9];
    let device = [8; 32];
    let sender = VerifiedChatSender {
        group: &group,
        author: &peer,
        device: &device,
    };
    let mut store = Store::open(&path, &owner).unwrap();
    crate::store::tests::catalog_group(&mut store, &group, &peer);
    let first = photo(1, 100);
    // An attachment cannot start a stranger's message request.
    assert!(!store
        .stage_chat_inbound(b"mls-1", &inbox(1), sender, &first, 1_000)
        .unwrap());
    assert!(store.current_chat_attachments(&peer).unwrap().is_empty());
    assert!(store.has_inbox(&inbox(1)).unwrap());
    open_photo_conversation(&mut store, sender, &group, &owner, &device);

    assert_future_photo_rejected(&mut store, sender);
    assert!(store
        .stage_chat_inbound(b"mls-4", &inbox(4), sender, &first, 1_000)
        .unwrap());
    assert!(!store
        .stage_chat_inbound(b"mls-4", &inbox(5), sender, &first, 1_000)
        .unwrap());
    assert_eq!(store.current_chat_attachments(&peer).unwrap().len(), 1);
    assert_eq!(store.sealed_device().unwrap().unwrap(), b"mls-4");
    let digest: Vec<u8> = store
        .db
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
            [inbox(4).id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(digest, sha256(&first.encode()));
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    let history = store.current_chat_attachments(&peer).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].key.id, [1; 16]);
    assert!(history[0].unread);
    assert_eq!(store.pending_acks().unwrap().len(), 5);
}

fn inbox(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [1; 32],
        id: [id; 16],
    }
}

fn unread(store: &Store, peer_id: &str) -> i64 {
    store
        .db
        .query_row(
            "SELECT COUNT(*) FROM `chat_messages` AS `m`
             JOIN `conversation_groups` AS `g` ON `m`.`group_id` = `g`.`group_id`
             WHERE `g`.`peer_id` = ?1 AND `m`.`unread` = 1",
            [peer_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn timed_text_rejects_future_clock_before_ack_and_observes_valid_clock() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let owner = Identity::try_generate().unwrap();
    let key = MessageKey {
        group: vec![3],
        author: Identity::try_generate().unwrap().peer_id(),
        id: [4; 16],
    };
    let device = [5; 32];
    let sender = VerifiedChatSender {
        group: &key.group,
        author: &key.author,
        device: &device,
    };
    let mut store = Store::open(&path, &owner).unwrap();
    crate::store::tests::catalog_group(&mut store, &key.group, &key.author);
    let timed = |hlc| Body::Text {
        id: key.id,
        hlc: Some(hlc),
        text: b"hello".to_vec(),
    };
    assert_future_clock_rejected(&mut store, sender, &key);
    let valid = Timestamp::new(61_000, 0);
    assert!(store
        .stage_chat_inbound(b"mls", &inbox(1), sender, &timed(valid.into()), 1_000)
        .unwrap());
    assert!(super::load_hlc(&store.db).unwrap() > valid);
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"hello"[..])
    );
    assert_eq!(store.pending_acks().unwrap(), vec![inbox(1)]);
    let stored = original_hlc(&store, &key);
    assert_eq!(stored, i64::try_from(u64::from(valid)).unwrap());
    assert!(matches!(
        store.stage_chat_inbound(
            b"other",
            &inbox(2),
            sender,
            &timed(Timestamp::new(61_000, 1).into()),
            1_000
        ),
        Err(Error::InboxConflict)
    ));
    assert!(!store.has_inbox(&inbox(2)).unwrap());
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert_eq!(original_hlc(&store, &key), stored);
}

fn assert_future_clock_rejected(store: &mut Store, sender: VerifiedChatSender, key: &MessageKey) {
    let timed = |hlc| Body::Text {
        id: key.id,
        hlc: Some(hlc),
        text: b"hello".to_vec(),
    };
    assert!(matches!(
        store.stage_chat_inbound(
            b"mls",
            &inbox(1),
            sender,
            &timed(Timestamp::new(61_001, 0).into()),
            1_000
        ),
        Err(Error::ClockSkew(_))
    ));
    assert!(matches!(
        store.stage_chat_inbound(b"mls", &inbox(1), sender, &timed(u64::MAX), 1_000),
        Err(Error::ClockRange)
    ));
    assert!(!store.has_inbox(&inbox(1)).unwrap());
}

fn original_hlc(store: &Store, key: &MessageKey) -> i64 {
    store
        .db
        .query_row(
            "SELECT `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&key.id[..]],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn chat_projection_and_delivery_commit_atomically() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = test_dir.path().join(format!(
        "dyapp-chat-inbound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::try_generate().unwrap();
    let author = Identity::try_generate().unwrap().peer_id();
    let key = MessageKey {
        group: vec![3],
        author,
        id: [4; 16],
    };
    let device = [5; 32];
    let sender = VerifiedChatSender {
        group: &key.group,
        author: &key.author,
        device: &device,
    };
    let original = Body::Text {
        id: key.id,
        hlc: None,
        text: b"private original".to_vec(),
    };
    let mut store = Store::open(&path, &owner).unwrap();
    crate::store::tests::catalog_group(&mut store, &key.group, &key.author);

    assert_original_delivery(&mut store, sender, &original);
    assert_conflicting_original(&mut store, sender, &key);
    apply_edit_and_delete(&mut store, sender, &key);
    drop(store);

    let store = Store::open(&path, &owner).unwrap();
    assert!(store.chat_message(&key).unwrap().unwrap().deleted);
    assert!(store.chat_message(&key).unwrap().unwrap().text.is_none());
    assert_eq!(
        store.pending_acks().unwrap(),
        vec![inbox(1), inbox(3), inbox(4)]
    );
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-3"[..])
    );
}

fn assert_original_delivery(store: &mut Store, sender: VerifiedChatSender, original: &Body) {
    assert!(store
        .stage_chat_inbound(b"mls-1", &inbox(1), sender, original, 1_000)
        .unwrap());
    assert!(!store
        .stage_chat_inbound(b"unused", &inbox(1), sender, original, 1_000)
        .unwrap());
    let saved: Vec<u8> = store
        .db
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
            [inbox(1).mailbox.as_slice(), inbox(1).id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(saved, sha256(&original.encode()));
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-1"[..])
    );
}

fn assert_conflicting_original(store: &mut Store, sender: VerifiedChatSender, key: &MessageKey) {
    let conflicting = Body::Text {
        id: key.id,
        hlc: None,
        text: b"other original".to_vec(),
    };
    assert!(matches!(
        store.stage_chat_inbound(b"mls-bad", &inbox(2), sender, &conflicting, 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(!store.has_inbox(&inbox(2)).unwrap());
    assert_eq!(store.pending_acks().unwrap(), vec![inbox(1)]);
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-1"[..])
    );
}

fn apply_edit_and_delete(store: &mut Store, sender: VerifiedChatSender, key: &MessageKey) {
    let edit = Body::Edit {
        target: key.id,
        operation: [6; 16],
        hlc: Timestamp::new(100, 0).into(),
        text: b"private replacement".to_vec(),
    };
    assert!(store
        .stage_chat_inbound(b"mls-2", &inbox(3), sender, &edit, 1_000)
        .unwrap());
    assert_eq!(
        store.chat_message(key).unwrap().unwrap().text.as_deref(),
        Some(&b"private replacement"[..])
    );
    let delete = Body::Delete {
        target: key.id,
        operation: [7; 16],
        hlc: Timestamp::new(200, 0).into(),
    };
    assert!(store
        .stage_chat_inbound(b"mls-3", &inbox(4), sender, &delete, 1_000)
        .unwrap());
}

#[test]
fn forwarded_text_keeps_its_marker_and_mls_author_after_restart() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let owner = Identity::try_generate().unwrap();
    let author = Identity::try_generate().unwrap().peer_id();
    let key = MessageKey {
        group: vec![8],
        author,
        id: [9; 16],
    };
    let device = [5; 32];
    let sender = VerifiedChatSender {
        group: &key.group,
        author: &key.author,
        device: &device,
    };
    let text = Body::text_with_id(key.id, b"forwarded words".to_vec());
    let forwarded = Body::Forwarded(Box::new(text.clone()));
    let mut store = Store::open(&path, &owner).unwrap();
    crate::store::tests::catalog_group(&mut store, &key.group, &key.author);
    assert!(store
        .stage_chat_inbound(b"mls", &inbox(1), sender, &forwarded, 1_000)
        .unwrap());
    let history = store.current_chat_messages(&key.author).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].key.author, key.author);
    assert!(history[0].forwarded);
    assert!(
        store.chat_rows().unwrap()[0]
            .last_message
            .as_ref()
            .unwrap()
            .forwarded
    );
    assert!(matches!(
        store.stage_chat_inbound(b"conflict", &inbox(2), sender, &text, 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(!store.has_inbox(&inbox(2)).unwrap());
    drop(store);
    let store = Store::open(&path, &owner).unwrap();
    assert!(store.current_chat_messages(&key.author).unwrap()[0].forwarded);
}

fn assert_late_original_survives_restart(
    path: &std::path::Path,
    owner: &Identity,
    key: &MessageKey,
    original: &Body,
) {
    let store = Store::open(path, owner).unwrap();
    let current = store.chat_message(key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    let original_hlc: i64 = store
        .db
        .query_row(
            "SELECT `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&key.id[..]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        original_hlc,
        i64::try_from(u64::from(Timestamp::new(150, 0))).unwrap()
    );
    assert_eq!(store.pending_acks().unwrap(), vec![inbox(1), inbox(2)]);
    let saved: Vec<u8> = store
        .db
        .query_row(
            "SELECT `payload` FROM `inbox` WHERE `mailbox` = ?1 AND `envelope_id` = ?2",
            [inbox(2).mailbox.as_slice(), inbox(2).id.as_slice()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(saved, sha256(&original.encode()));
}

#[test]
fn late_original_after_delete_keeps_placeholder_and_ack_intent() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = test_dir.path().join(format!(
        "dyapp-chat-late-original-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::try_generate().unwrap();
    let key = MessageKey {
        group: vec![3],
        author: Identity::try_generate().unwrap().peer_id(),
        id: [4; 16],
    };
    let device = [5; 32];
    let sender = VerifiedChatSender {
        group: &key.group,
        author: &key.author,
        device: &device,
    };
    let delete = Body::Delete {
        target: key.id,
        operation: [7; 16],
        hlc: Timestamp::new(200, 0).into(),
    };
    let original = Body::Text {
        id: key.id,
        hlc: Some(Timestamp::new(150, 0).into()),
        text: b"late private original".to_vec(),
    };
    let mut store = Store::open(&path, &owner).unwrap();
    crate::store::tests::catalog_group(&mut store, &key.group, &key.author);
    assert!(store
        .stage_chat_inbound(b"mls-delete", &inbox(1), sender, &delete, 1_000)
        .unwrap());
    let current = store.chat_message(&key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    assert_eq!(store.pending_acks().unwrap(), vec![inbox(1)]);

    // The body was authenticated by MLS but its original arrived after the delete.
    assert!(store
        .stage_chat_inbound(b"mls-original", &inbox(2), sender, &original, 1_000)
        .unwrap());
    let current = store.chat_message(&key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    assert_eq!(store.pending_acks().unwrap(), vec![inbox(1), inbox(2)]);
    assert!(!store
        .stage_chat_inbound(b"unused", &inbox(2), sender, &original, 1_000)
        .unwrap());
    drop(store);

    assert_late_original_survives_restart(&path, &owner, &key, &original);
}

#[test]
fn unread_is_local_to_a_pair_and_survives_late_delivery_and_restart() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let owner = Identity::try_generate().unwrap();
    let own = owner.peer_id();
    let peer = Identity::try_generate().unwrap().peer_id();
    let other = Identity::try_generate().unwrap().peer_id();
    let (group_a, group_b, group_other, device) = ([3], [4], [5], [6; 32]);
    let mut store = Store::open(&path, &owner).unwrap();
    stage_unread_history(
        &mut store,
        &own,
        &peer,
        &other,
        (&group_a, &group_b, &group_other),
        &device,
    );

    assert_unread_updates(&mut store, &peer, &other, (&group_a, &group_b), &device);
    drop(store);

    let mut store = Store::open(&path, &owner).unwrap();
    assert_eq!((unread(&store, &peer), unread(&store, &other)), (0, 1));
    assert_eq!(store.mark_chat_read(&other).unwrap(), 1);
    assert!(matches!(
        Store::open(&path, &Identity::try_generate().unwrap()),
        Err(Error::WrongIdentity)
    ));
}

fn timed_text(id: u8, time: u64) -> Body {
    Body::Text {
        id: [id; 16],
        hlc: Some(Timestamp::new(time, 0).into()),
        text: vec![id],
    }
}

fn stage_unread_history(
    store: &mut Store,
    own: &str,
    peer: &str,
    other: &str,
    groups: (&[u8], &[u8], &[u8]),
    device: &[u8; 32],
) {
    let (group_a, group_b, group_other) = groups;
    let from = |group, author| VerifiedChatSender {
        group,
        author,
        device,
    };
    for (group, author) in [(group_a, peer), (group_b, peer), (group_other, other)] {
        crate::store::tests::catalog_group(store, group, author);
    }
    assert!(!store.send_read_receipts().unwrap());
    store
        .stage_chat_inbound(
            b"mls",
            &inbox(1),
            from(group_a, peer),
            &timed_text(1, 200),
            1_000,
        )
        .unwrap();
    assert_eq!(unread(store, peer), 1);
    assert_eq!(store.mark_chat_read(peer).unwrap(), 1);
    // An own text from another device opens the request without making it unread.
    for (id, group, author, time) in [
        (2, group_a, own, 210),
        (3, group_b, peer, 100),
        (4, group_other, other, 50),
    ] {
        store
            .stage_chat_inbound(
                b"mls",
                &inbox(id),
                from(group, author),
                &timed_text(id, time),
                1_000,
            )
            .unwrap();
    }
    assert_eq!((unread(store, peer), unread(store, other)), (1, 1));
}

fn assert_unread_updates(
    store: &mut Store,
    peer: &str,
    other: &str,
    groups: (&[u8], &[u8]),
    device: &[u8; 32],
) {
    let (group_a, group_b) = groups;
    let from = |group| VerifiedChatSender {
        group,
        author: peer,
        device,
    };
    let edit = Body::Edit {
        target: [3; 16],
        operation: [7; 16],
        hlc: Timestamp::new(300, 0).into(),
        text: b"edited".to_vec(),
    };
    let delete = Body::Delete {
        target: [1; 16],
        operation: [8; 16],
        hlc: Timestamp::new(400, 0).into(),
    };
    for (id, group, body) in [(5, group_b, edit), (6, group_a, delete)] {
        store
            .stage_chat_inbound(b"mls", &inbox(id), from(group), &body, 1_000)
            .unwrap();
    }
    assert_eq!(unread(store, peer), 1);
    store.set_send_read_receipts(true).unwrap();
    assert_eq!(store.mark_chat_read(peer).unwrap(), 1);
    assert_eq!(store.mark_chat_read(peer).unwrap(), 0);
    assert!(store.send_read_receipts().unwrap());
    assert_eq!(unread(store, other), 1);
    // A late original with an older HLC still starts unread after a prior mark-on-view.
    store
        .stage_chat_inbound(b"mls", &inbox(7), from(group_b), &timed_text(9, 80), 1_000)
        .unwrap();
    assert_eq!(unread(store, peer), 1);
    assert_eq!(store.clear_chat(group_b).unwrap(), 2);
    assert_eq!(unread(store, peer), 0);
}
