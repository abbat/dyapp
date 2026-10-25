use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::messages::MessageKey;
use dyapp_client::store::Store;

#[tokio::test]
async fn two_sessions_converge_on_edit_and_terminal_delete() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let (alice_path, bob_path) = (store_path("alice-edits"), store_path("bob-edits"));
    let (alice_key, bob_key) = ([1; 32], [2; 32]);
    let alice_owner = alice.identity.peer_id();
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [&alice_owner, &bob_owner] {
        profiles.push(alice.node.get_profile(owner).await.unwrap());
    }
    Store::open(&bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&bob_key).unwrap(), &[])
        .unwrap();
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    let group_id = group.id().to_vec();
    let welcome = welcome
        .into_iter()
        .map(|delivery| dyapp_client::prepare_put(&delivery.mailbox, delivery.mls).unwrap())
        .collect::<Vec<_>>();
    Store::open(&alice_path, &alice.identity)
        .unwrap()
        .stage_group(
            &group,
            &profiles[1],
            &alice.device.seal(&alice_key).unwrap(),
            &welcome,
        )
        .unwrap();
    let mut alice_session = Session::open(
        Store::open(&alice_path, &alice.identity).unwrap(),
        &alice_key,
    )
    .unwrap();
    let mut bob_session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &bob_key).unwrap();
    alice.node.put_prepared(&welcome[0]).await.unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Joined(id)] if id == &group_id
    ));

    let (id, text_writes) = alice_session
        .stage_text(&group_id, b"private first text", &alice_key)
        .unwrap();
    let key = MessageKey {
        group: group_id.clone(),
        author: alice_owner,
        id,
    };
    alice.node.put_prepared(&text_writes[0]).await.unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Message { body: Body::Text { id: seen, .. }, .. }] if seen == &id
    ));
    assert_eq!(
        bob_session
            .chat_message(&key)
            .unwrap()
            .unwrap()
            .text
            .as_deref(),
        Some(&b"private first text"[..])
    );

    alice_session
        .send_edit(
            std::slice::from_mut(&mut alice.node),
            &group_id,
            id,
            b"private replacement",
            &alice_key,
        )
        .await
        .unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Message { body: Body::Edit { target, .. }, .. }] if target == &id
    ));
    for session in [&alice_session, &bob_session] {
        let current = session.chat_message(&key).unwrap().unwrap();
        assert!(current.edited && !current.deleted);
        assert_eq!(current.text.as_deref(), Some(&b"private replacement"[..]));
    }

    alice_session
        .send_delete(
            std::slice::from_mut(&mut alice.node),
            &group_id,
            id,
            &alice_key,
        )
        .await
        .unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Message { body: Body::Delete { target, .. }, .. }] if target == &id
    ));
    for session in [&alice_session, &bob_session] {
        let current = session.chat_message(&key).unwrap().unwrap();
        assert!(current.edited && current.deleted && current.text.is_none());
    }
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());
    let (count, shortest, longest): (i64, i64, i64) =
        rusqlite::Connection::open(&bob_path)
            .unwrap()
            .query_row(
                "SELECT COUNT(*), MIN(length(`payload`)), MAX(length(`payload`)) FROM `inbox` WHERE `kind` = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
    assert_eq!((count, shortest, longest), (3, 32, 32));
}
