use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::messages::MessageKey;
use dyapp_client::store::Store;
use dyapp_client::PreparedEnvelope;
use dyapp_profile::VerifiedProfile;

#[tokio::test]
async fn two_sessions_converge_on_edit_and_terminal_delete() {
    let EditFixture {
        mut alice,
        mut bob,
        profiles,
        alice_path,
        bob_path,
        group_id,
        mut alice_session,
        mut bob_session,
    } = Box::pin(open_edit_fixture("edits")).await;
    let alice_owner = alice.identity.peer_id();
    let (alice_key, bob_key) = (ALICE_EDIT_SEAL, BOB_EDIT_SEAL);

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
    // The recipient's signed ACK removed the original from this cooperating node.
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    // A local operation must target this sender's text in this exact group.
    assert!(matches!(
        alice_session.stage_edit(b"wrong group", id, b"forged", &alice_key),
        Err(dyapp_client::session::Error::UnknownMessage)
    ));
    assert!(matches!(
        bob_session.stage_delete(&group_id, id, &bob_key),
        Err(dyapp_client::session::Error::UnknownMessage)
    ));
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);

    // Stage while offline, reopen the saved MLS state, then retry the exact signed write.
    let (edit_operation, edit_writes) = alice_session
        .stage_edit(&group_id, id, b"private replacement", &alice_key)
        .unwrap();
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    drop(alice_session);
    let mut alice_session = Session::open(
        Store::open(&alice_path, &alice.identity).unwrap(),
        &alice_key,
    )
    .unwrap();
    assert!(alice_session
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .any(|write| write.as_bytes() == edit_writes[0].as_bytes()));
    let now_ms = super::now_ms();
    alice_session
        .retry_due(std::slice::from_mut(&mut alice.node), now_ms, &alice_key)
        .await
        .unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Message { body: Body::Edit { target, operation, .. }, .. }]
            if target == &id && operation == &edit_operation
    ));
    for session in [&alice_session, &bob_session] {
        let current = session.chat_message(&key).unwrap().unwrap();
        assert!(current.edited && !current.deleted);
        assert_eq!(current.text.as_deref(), Some(&b"private replacement"[..]));
    }

    let (delete_operation, delete_writes) = alice_session
        .stage_delete(&group_id, id, &alice_key)
        .unwrap();
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    drop(alice_session);
    let mut alice_session = Session::open(
        Store::open(&alice_path, &alice.identity).unwrap(),
        &alice_key,
    )
    .unwrap();
    assert!(alice_session
        .due_outbound(i64::MAX)
        .unwrap()
        .iter()
        .any(|write| write.as_bytes() == delete_writes[0].as_bytes()));
    alice_session
        .retry_due(std::slice::from_mut(&mut alice.node), now_ms, &alice_key)
        .await
        .unwrap();
    assert!(matches!(
        bob_session
            .receive(std::slice::from_mut(&mut bob.node), &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap()
            .as_slice(),
        [Event::Message { body: Body::Delete { target, operation, .. }, .. }]
            if target == &id && operation == &delete_operation
    ));
    for session in [&alice_session, &bob_session] {
        let current = session.chat_message(&key).unwrap().unwrap();
        assert!(current.edited && current.deleted && current.text.is_none());
    }
    assert!(matches!(
        alice_session.stage_edit(&group_id, id, b"revive", &alice_key),
        Err(dyapp_client::session::Error::UnknownMessage)
    ));
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    // The node may accept the same signed envelope after an ACK; inbox dedupe shows nothing.
    alice.node.put_prepared(&delete_writes[0]).await.unwrap();
    let checked_items = bob_session
        .receive(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &bob_key,
        )
        .await
        .unwrap();
    assert_eq!(checked_items.len(), 0);
    let current = bob_session.chat_message(&key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    // A node may receive the original again after its ACK; replay stays invisible and is ACKed.
    alice.node.put_prepared(&text_writes[0]).await.unwrap();
    let checked_items = bob_session
        .receive(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &bob_key,
        )
        .await
        .unwrap();
    assert_eq!(checked_items.len(), 0);
    let current = bob_session.chat_message(&key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
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

const ALICE_EDIT_SEAL: [u8; 32] = [1; 32];
const BOB_EDIT_SEAL: [u8; 32] = [2; 32];

struct EditFixture {
    alice: Person,
    bob: Person,
    profiles: Vec<VerifiedProfile>,
    alice_path: std::path::PathBuf,
    bob_path: std::path::PathBuf,
    group_id: Vec<u8>,
    alice_session: Session,
    bob_session: Session,
}

async fn open_edit_fixture(label: &str) -> EditFixture {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let alice_owner = alice.identity.peer_id();
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [&alice_owner, &bob_owner] {
        profiles.push(alice.node.get_profile(owner).await.unwrap());
    }
    let alice_path = store_path(&format!("alice-{label}"));
    let bob_path = store_path(&format!("bob-{label}"));
    let (group_id, welcome) =
        stage_edit_group(&mut alice, &bob, &profiles, &alice_path, &bob_path).await;
    let (alice_session, bob_session) = open_edit_sessions(
        &mut alice,
        &mut bob,
        &profiles,
        &alice_path,
        &bob_path,
        &group_id,
        &welcome,
    )
    .await;
    EditFixture {
        alice,
        bob,
        profiles,
        alice_path,
        bob_path,
        group_id,
        alice_session,
        bob_session,
    }
}

async fn stage_edit_group(
    alice: &mut Person,
    bob: &Person,
    profiles: &[VerifiedProfile],
    alice_path: &std::path::Path,
    bob_path: &std::path::Path,
) -> (Vec<u8>, Vec<PreparedEnvelope>) {
    Store::open(bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&BOB_EDIT_SEAL).unwrap(), &[])
        .unwrap();
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, profiles)
        .await
        .unwrap();
    let group_id = group.id().to_vec();
    let welcome = welcome
        .into_iter()
        .map(|delivery| dyapp_client::prepare_put(&delivery.mailbox, delivery.mls).unwrap())
        .collect::<Vec<_>>();
    Store::open(alice_path, &alice.identity)
        .unwrap()
        .stage_group(
            &group,
            &profiles[1],
            &alice.device.seal(&ALICE_EDIT_SEAL).unwrap(),
            &welcome,
        )
        .unwrap();
    (group_id, welcome)
}

async fn open_edit_sessions(
    alice: &mut Person,
    bob: &mut Person,
    profiles: &[VerifiedProfile],
    alice_path: &std::path::Path,
    bob_path: &std::path::Path,
    group_id: &[u8],
    welcome: &[PreparedEnvelope],
) -> (Session, Session) {
    let alice_session = Session::open(
        Store::open(alice_path, &alice.identity).unwrap(),
        &ALICE_EDIT_SEAL,
    )
    .unwrap();
    let mut bob_session = Session::open(
        Store::open(bob_path, &bob.identity).unwrap(),
        &BOB_EDIT_SEAL,
    )
    .unwrap();
    alice.node.put_prepared(&welcome[0]).await.unwrap();
    assert!(matches!(
        bob_session
            .receive(
                std::slice::from_mut(&mut bob.node),
                &bob.device_key,
                profiles,
                &BOB_EDIT_SEAL,
            )
            .await
            .unwrap()
            .as_slice(),
        [Event::Joined(id)] if id == group_id
    ));
    (alice_session, bob_session)
}

#[tokio::test]
async fn offline_recipient_acknowledges_original_and_delete_after_reconnect() {
    let EditFixture {
        mut alice,
        mut bob,
        profiles,
        group_id,
        mut alice_session,
        mut bob_session,
        ..
    } = Box::pin(open_edit_fixture("offline-delete")).await;
    let alice_owner = alice.identity.peer_id();
    let (alice_key, bob_key) = (ALICE_EDIT_SEAL, BOB_EDIT_SEAL);

    let (id, text_writes) = alice_session
        .stage_text(&group_id, b"unread original", &alice_key)
        .unwrap();
    alice.node.put_prepared(&text_writes[0]).await.unwrap();
    let (delete_operation, delete_writes) = alice_session
        .stage_delete(&group_id, id, &alice_key)
        .unwrap();
    alice.node.put_prepared(&delete_writes[0]).await.unwrap();
    // An offline recipient has signed no ACK; the node still holds both ciphertext envelopes.
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 2);
    let events = bob_session
        .receive(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &bob_key,
        )
        .await
        .unwrap();
    assert!(matches!(events.as_slice(),
        [Event::Message { body: Body::Text { id: original, .. }, .. },
         Event::Message { body: Body::Delete { target, operation, .. }, .. }]
        if original == &id && target == &id && operation == &delete_operation));
    let key = MessageKey {
        group: group_id,
        author: alice_owner,
        id,
    };
    let current = bob_session.chat_message(&key).unwrap().unwrap();
    assert!(current.deleted && current.text.is_none());
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
}
