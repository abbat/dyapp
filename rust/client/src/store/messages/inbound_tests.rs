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
        text: b"private original".to_vec(),
    };
    let mut store = Store::open(&path, &owner).unwrap();

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
