use super::{Error, InboxKey, InboxKind, Store};
use dyapp_identity::Identity;
use rusqlite::params;

fn assert_projection_rollback(store: &mut Store, key: &InboxKey) {
    assert!(matches!(
        store.stage_inbound_with(b"mls-v1", key, InboxKind::Message, b"digest", None, |tx| {
            tx.execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
                params!["chat-marker", b"one"],
            )?;
            Err(Error::InboxConflict)
        }),
        Err(Error::InboxConflict)
    ));
    assert!(!store.has_inbox(key).unwrap());
    assert_eq!(store.pending_acks().unwrap(), [] as [InboxKey; 0]);
    assert!(store.sealed_device().unwrap().is_none());
}

#[test]
fn projection_hook_shares_inbox_mls_and_ack_transaction() {
    let path = crate::store::tests::temp_db_path();
    let owner = Identity::generate();
    let key = InboxKey {
        mailbox: [1; 32],
        id: [2; 16],
    };
    let mut store = Store::open(&path, &owner).unwrap();
    assert_projection_rollback(&mut store, &key);

    assert!(store
        .stage_inbound_with(b"mls-v1", &key, InboxKind::Message, b"digest", None, |tx| {
            tx.execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
                params!["chat-marker", b"two"],
            )?;
            Ok(())
        })
        .unwrap());
    assert!(!store
        .stage_inbound_with(
            b"mls-should-not-replace",
            &key,
            InboxKind::Message,
            b"digest",
            None,
            |_| { panic!("duplicate envelope applied its projection") }
        )
        .unwrap());
    assert_eq!(
        store.sealed_device().unwrap().as_deref(),
        Some(&b"mls-v1"[..])
    );
    drop(store);

    let store = Store::open(&path, &owner).unwrap();
    assert_eq!(store.pending_acks().unwrap(), vec![key]);
    let marker: Vec<u8> = store
        .db
        .query_row(
            "SELECT `value` FROM `settings` WHERE `key` = ?1",
            ["chat-marker"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marker, b"two");
}
