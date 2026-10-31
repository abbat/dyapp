use super::chat::Person;
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::prepare_put;
use dyapp_client::session::{Error, Session};
use dyapp_client::store::messages::MessageKey;
use dyapp_client::store::Store;
use rusqlite::{params, Connection};

#[tokio::test]
async fn ack_marker_failure_keeps_projection_and_retries_without_duplicate_event() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.require_write_tokens = false;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let profiles = [
        alice
            .node
            .get_profile(&alice.identity.peer_id())
            .await
            .unwrap(),
        bob.node.get_profile(&bob_owner).await.unwrap(),
    ];
    let path = protocol_build_path("bob-ack-failure").with_extension("sqlite3");
    let seal_key = [8; 32];
    Store::open(&path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&seal_key).unwrap(), &[])
        .unwrap();
    let alice_mailbox = alice.mailbox();
    let (mut group, welcome) =
        chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
            .await
            .unwrap();
    let group_id = group.id().to_vec();
    let welcome = prepare_put(&welcome[0].mailbox, welcome[0].mls.clone()).unwrap();
    alice.node.put_prepared(&welcome).await.unwrap();
    let mut session = Session::open(Store::open(&path, &bob.identity).unwrap(), &seal_key).unwrap();
    let (events, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &seal_key,
        )
        .await
        .unwrap();
    assert!(!more);
    assert!(events.iter().any(|event| matches!(event, Event::Joined(_))));

    for _ in 0..20 {
        alice.node.put(&bob.mailbox(), vec![0xff]).await.unwrap();
    }
    let text = chat::message(
        &mut group,
        &alice.device,
        &Body::text_with_id([3; 16], b"committed once".to_vec()),
    )
    .unwrap();
    let text = prepare_put(&text[0].mailbox, text[0].mls.clone()).unwrap();
    alice.node.put_prepared(&text).await.unwrap();
    let (_, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &seal_key,
        )
        .await
        .unwrap();
    assert!(more);
    let cursor = session
        .store()
        .mailbox_cursor_for(bob.node.peer())
        .unwrap()
        .unwrap();

    // The node ACK succeeds; only the local mark fails after the inbox projection commits.
    let db = Connection::open(&path).unwrap();
    db.execute(
        "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
        params!["test_fail_ack_marker", [1u8]],
    )
    .unwrap();
    db.execute_batch(
        "CREATE TRIGGER `test_fail_ack_marker` BEFORE UPDATE OF `ack_pending` ON `inbox`
         WHEN (SELECT `value` FROM `settings` WHERE `key` = 'test_fail_ack_marker') = x'01'
         BEGIN SELECT RAISE(ABORT, 'injected ACK marker failure'); END",
    )
    .unwrap();
    assert!(matches!(
        session
            .receive_page(
                std::slice::from_mut(&mut bob.node),
                &bob.device_key,
                &profiles,
                &seal_key,
            )
            .await,
        Err(Error::Store(_))
    ));
    drop(session);
    let store = Store::open(&path, &bob.identity).unwrap();
    let key = MessageKey {
        group: group_id,
        author: alice.identity.peer_id(),
        id: [3; 16],
    };
    assert_eq!(
        store.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"committed once"[..])
    );
    assert_eq!(store.pending_acks().unwrap().len(), 1);
    assert_eq!(
        store.mailbox_cursor_for(bob.node.peer()).unwrap(),
        Some(cursor)
    );
    drop(store);

    db.execute(
        "UPDATE `settings` SET `value` = ?1 WHERE `key` = ?2",
        params![[0u8], "test_fail_ack_marker"],
    )
    .unwrap();
    alice.node.put_prepared(&text).await.unwrap();
    let mut session = Session::open(Store::open(&path, &bob.identity).unwrap(), &seal_key).unwrap();
    let (events, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &seal_key,
        )
        .await
        .unwrap();
    assert!(!more);
    assert!(events.is_empty());
    assert!(session.store().pending_acks().unwrap().is_empty());
    assert_eq!(
        session.store().mailbox_cursor_for(bob.node.peer()).unwrap(),
        None
    );
    assert_eq!(
        session.chat_message(&key).unwrap().unwrap().text.as_deref(),
        Some(&b"committed once"[..])
    );
}
