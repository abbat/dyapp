use super::{MessageKey, Store};
use crate::store::Error;
use dyapp_identity::Identity;

#[test]
fn text_projection_survives_restart_and_refuses_conflicts() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = test_dir.path().join(format!(
        "dyapp-chat-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let alice = Identity::try_generate().unwrap();
    let bob = Identity::try_generate().unwrap();
    let key = MessageKey {
        group: vec![3],
        author: alice.peer_id(),
        id: [4; 16],
    };
    let mut store = Store::open(&path, &alice).unwrap();
    assert!(store.chat_message(&key).unwrap().is_none());
    assert!(store.record_chat_text(&key, b"hello").unwrap());
    assert!(!store.record_chat_text(&key, b"hello").unwrap());
    assert!(matches!(
        store.record_chat_text(&key, b"other"),
        Err(Error::InboxConflict)
    ));
    drop(store);
    assert!(matches!(
        Store::open(&path, &bob),
        Err(Error::WrongIdentity)
    ));
    let store = Store::open(&path, &alice).unwrap();
    let message = store.chat_message(&key).unwrap().unwrap();
    assert_eq!(message.text.as_deref(), Some(&b"hello"[..]));
    assert!(!message.edited && !message.deleted);
}

#[test]
fn legacy_arrival_order_is_not_reused_after_clear_and_restart() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let path = test_dir.path().join("client-1.db");
    let owner = Identity::try_generate().unwrap();
    let key = |id| MessageKey {
        group: vec![3],
        author: owner.peer_id(),
        id: [id; 16],
    };
    let mut store = Store::open(&path, &owner).unwrap();
    assert!(store.record_chat_text(&key(1), b"first").unwrap());
    assert!(store.record_chat_text(&key(2), b"second").unwrap());
    let before: (i64, i64, Option<i64>) = store.db.query_row(
        "SELECT `rowid`, `arrival_order`, `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
        [&[2u8; 16][..]],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(before.0, before.1);
    assert_eq!(before.2, None);
    assert_eq!(store.clear_chat(&[3]).unwrap(), 2);
    drop(store);

    let mut store = Store::open(&path, &owner).unwrap();
    assert!(store.record_chat_text(&key(3), b"third").unwrap());
    let after: (i64, Option<i64>) = store
        .db
        .query_row(
            "SELECT `arrival_order`, `original_hlc` FROM `chat_messages` WHERE `message_id` = ?1",
            [&[3u8; 16][..]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(after.0 > before.1);
    assert_eq!(after.1, None);
    assert!(!store.record_chat_text(&key(3), b"third").unwrap());
}
