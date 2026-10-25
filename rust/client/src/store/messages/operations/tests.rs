use super::{Body, Error, MessageKey, Store, Timestamp, MAX_PENDING_OPERATIONS};
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
    // This remains pending under the other author's key; it cannot change Alice's text.
    assert!(store
        .apply_chat_operation(&wrong_author, &[2; 32], &newer, 1_000)
        .unwrap());
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"tie winner"[..])
    );
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

#[test]
fn early_edits_resolve_once_and_discard_pending_text() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-early-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let author = Identity::generate();
    let key = MessageKey {
        group: vec![8],
        author: author.peer_id(),
        id: [9; 16],
    };
    let old = edit(key.id, 1, 100, b"old edit");
    let new = edit(key.id, 2, 200, b"new edit");
    let mut store = Store::open(&path, &author).unwrap();
    assert!(store
        .apply_chat_operation(&key, &[1; 32], &new, 1_000)
        .unwrap());
    assert!(store
        .apply_chat_operation(&key, &[2; 32], &old, 1_000)
        .unwrap());
    assert!(store.chat_message(&key).unwrap().is_none());
    drop(store);
    let mut store = Store::open(&path, &author).unwrap();
    assert!(store.record_chat_text(&key, b"original").unwrap());
    let current = store.chat_message(&key).unwrap().unwrap();
    assert_eq!(current.text.as_deref(), Some(&b"new edit"[..]));
    assert!(current.edited && !current.deleted);
    assert!(!store.record_chat_text(&key, b"original").unwrap());
    assert!(!store
        .apply_chat_operation(&key, &[1; 32], &new, 1_000)
        .unwrap());
    let pending: i64 = store.db.query_row(
        "SELECT COUNT(*) FROM `chat_operations` WHERE `pending` = 1 OR `pending_text` IS NOT NULL",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(pending, 0);
}

#[test]
fn early_delete_is_terminal_and_pending_budget_is_global() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-chat-bound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let author = Identity::generate();
    let other = Identity::generate();
    let key = MessageKey {
        group: vec![8],
        author: author.peer_id(),
        id: [9; 16],
    };
    let mut store = Store::open(&path, &author).unwrap();
    assert!(store
        .apply_chat_operation(
            &key,
            &[1; 32],
            &edit(key.id, 1, 100, b"do not retain"),
            1_000
        )
        .unwrap());
    assert!(store
        .apply_chat_operation(&key, &[1; 32], &delete(key.id, 2, 200), 1_000)
        .unwrap());
    assert!(store.chat_message(&key).unwrap().unwrap().deleted);
    assert!(store
        .apply_chat_operation(
            &key,
            &[1; 32],
            &edit(key.id, 3, 300, b"cannot revive"),
            1_000
        )
        .unwrap());
    let retained: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM `chat_operations` WHERE `pending_text` IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, 0);
    // A different verified author cannot affect this author's placeholder.
    let wrong = MessageKey {
        author: other.peer_id(),
        ..key.clone()
    };
    assert!(store
        .apply_chat_operation(&wrong, &[1; 32], &edit(key.id, 4, 400, b"wrong"), 1_000)
        .unwrap());
    assert!(store.chat_message(&key).unwrap().unwrap().deleted);
    drop(store);
    let mut store = Store::open(&path, &author).unwrap();
    assert!(!store.record_chat_text(&key, b"original").unwrap());
    assert!(store.chat_message(&key).unwrap().unwrap().deleted);
    let pending: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM `chat_operations` WHERE `pending` = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(pending, 1); // the other author's unresolved edit

    // Distinct target IDs cannot bypass the profile-wide cap.
    for i in 0..(MAX_PENDING_OPERATIONS - pending) {
        let target = [i as u8 + 100; 16];
        let unknown = MessageKey {
            id: target,
            ..key.clone()
        };
        assert!(store
            .apply_chat_operation(
                &unknown,
                &[1; 32],
                &edit(target, i as u8 + 10, 500, b"pending"),
                1_000
            )
            .unwrap());
    }
    let overflow = MessageKey {
        id: [250; 16],
        ..key
    };
    assert!(matches!(
        store.apply_chat_operation(
            &overflow,
            &[1; 32],
            &edit(overflow.id, 250, 500, b"overflow"),
            1_000
        ),
        Err(Error::InboxConflict)
    ));
    assert!(store.chat_message(&overflow).unwrap().is_none());
    let count: i64 = store
        .db
        .query_row(
            "SELECT COUNT(*) FROM `chat_operations` WHERE `pending` = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, MAX_PENDING_OPERATIONS);
}
