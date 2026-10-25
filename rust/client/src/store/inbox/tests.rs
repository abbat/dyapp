use super::{Error, InboxKey, InboxKind, Store};
use dyapp_identity::Identity;
use rusqlite::params;

#[test]
fn projection_hook_shares_inbox_mls_and_ack_transaction() {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    let path = std::env::temp_dir().join(format!(
        "dyapp-inbound-{:032x}.db",
        u128::from_be_bytes(random)
    ));
    let owner = Identity::generate();
    let key = InboxKey {
        mailbox: [1; 32],
        id: [2; 16],
    };
    let mut store = Store::open(&path, &owner).unwrap();

    assert!(matches!(
        store.stage_inbound_with(b"mls-v1", &key, InboxKind::Message, b"digest", None, |tx| {
            tx.execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
                params!["chat-marker", b"one"],
            )?;
            Err(Error::InboxConflict)
        }),
        Err(Error::InboxConflict)
    ));
    assert!(!store.has_inbox(&key).unwrap());
    assert!(store.pending_acks().unwrap().is_empty());
    assert!(store.sealed_device().unwrap().is_none());

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
