use super::{Body, Error, MessageKey, Store, Timestamp};
use dyapp_identity::Identity;

fn edit(target: [u8; 16], operation: u8, wall_ms: u64, text: &[u8]) -> Body {
    Body::Edit {
        target,
        operation: [operation; 16],
        hlc: Timestamp::new(wall_ms, 0).into(),
        text: text.to_vec(),
    }
}

fn delete(target: [u8; 16], operation: u8, wall_ms: u64) -> Body {
    Body::Delete {
        target,
        operation: [operation; 16],
        hlc: Timestamp::new(wall_ms, 0).into(),
    }
}

#[test]
fn ordered_operations_replay_and_terminal_delete_survive_restart() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-ops-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let alice = Identity::generate();
    let bob = Identity::generate();
    let key = MessageKey {
        group: vec![3],
        author: alice.peer_id(),
        id: [4; 16],
    };
    let mut store = Store::open(&path, &alice).unwrap();
    assert!(store.record_chat_text(&key, b"original").unwrap());

    let newer = edit(key.id, 1, 200, b"newer");
    assert!(store
        .apply_chat_operation(&key, &[2; 32], &newer, 1_000)
        .unwrap());
    assert!(!store
        .apply_chat_operation(&key, &[2; 32], &newer, 1_000)
        .unwrap());
    assert!(matches!(
        store.apply_chat_operation(&key, &[3; 32], &newer, 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(matches!(
        store.apply_chat_operation(&key, &[2; 31], &newer, 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(matches!(
        store.apply_chat_operation(&key, &[2; 32], &edit(key.id, 1, 200, b"forged"), 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(!store
        .apply_chat_operation(&key, &[9; 32], &edit(key.id, 2, 100, b"older"), 1_000)
        .unwrap());
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"newer"[..])
    );
    assert!(!store.record_chat_text(&key, b"original").unwrap());
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"newer"[..])
    );

    // The sending device breaks equal HLC ties, followed by operation ID.
    assert!(store
        .apply_chat_operation(&key, &[3; 32], &edit(key.id, 3, 200, b"tie winner"), 1_000)
        .unwrap());
    assert!(!store
        .apply_chat_operation(&key, &[1; 32], &edit(key.id, 4, 200, b"tie loser"), 1_000)
        .unwrap());
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"tie winner"[..])
    );

    let wrong_author = MessageKey {
        author: bob.peer_id(),
        ..key.clone()
    };
    assert!(matches!(
        store.apply_chat_operation(&wrong_author, &[2; 32], &newer, 1_000),
        Err(Error::InboxConflict)
    ));
    assert!(matches!(
        store.apply_chat_operation(&key, &[2; 32], &edit(key.id, 5, 100_000, b"future"), 1_000),
        Err(Error::ClockSkew(_))
    ));
    assert!(store
        .apply_chat_operation(&key, &[1; 32], &delete(key.id, 6, 50), 1_000)
        .unwrap());
    assert!(!store
        .apply_chat_operation(&key, &[3; 32], &edit(key.id, 7, 500, b"revive"), 1_000)
        .unwrap());
    assert!(!store.record_chat_text(&key, b"original").unwrap());
    assert!(matches!(
        store.record_chat_text(&key, b"forged original"),
        Err(Error::InboxConflict)
    ));
    let message = store.chat_message(&key).unwrap().unwrap();
    assert!(message.text.is_none() && message.deleted);
    drop(store);

    let mut store = Store::open(&path, &alice).unwrap();
    assert!(!store.record_chat_text(&key, b"original").unwrap());
    let message = store.chat_message(&key).unwrap().unwrap();
    assert!(message.text.is_none() && message.deleted);
}
