use super::{Body, Error, InboxKey, MessageKey, Store, VerifiedChatSender};
use dyapp_identity::{sha256, Identity};
use dyapp_messaging::hlc::Timestamp;

fn inbox(id: u8) -> InboxKey {
    InboxKey {
        mailbox: [1; 32],
        id: [id; 16],
    }
}

#[test]
fn timed_text_rejects_future_clock_before_ack_and_observes_valid_clock() {
    let path = crate::store::tests::temp_db_path();
    let owner = Identity::generate();
    let key = MessageKey {
        group: vec![3],
        author: Identity::generate().peer_id(),
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
    let stored: i64 = store
        .db
        .query_row(
            "SELECT `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&key.id[..]],
            |row| row.get(0),
        )
        .unwrap();
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
    let restarted: i64 = store
        .db
        .query_row(
            "SELECT `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&key.id[..]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restarted, stored);
}

#[test]
fn chat_projection_and_delivery_commit_atomically() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-inbound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::generate();
    let author = Identity::generate().peer_id();
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

    assert!(store
        .stage_chat_inbound(b"mls-1", &inbox(1), sender, &original, 1_000)
        .unwrap());
    assert!(!store
        .stage_chat_inbound(b"unused", &inbox(1), sender, &original, 1_000)
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
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
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

#[test]
fn late_original_after_delete_keeps_placeholder_and_ack_intent() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-late-original-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::generate();
    let key = MessageKey {
        group: vec![3],
        author: Identity::generate().peer_id(),
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

    let store = Store::open(&path, &owner).unwrap();
    let current = store.chat_message(&key).unwrap().unwrap();
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
