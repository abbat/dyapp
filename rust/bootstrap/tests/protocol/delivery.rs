//! Resends through another node once a receipt shows the first one dropped a write.
use super::chat::Person;
use super::*;
use dyapp_client::chat::{self, Body};
use dyapp_client::interactions::Kind;
use dyapp_client::session::{Error, Session};
use dyapp_client::store::Store;
use std::slice::from_mut;

const SEAL: [u8; 32] = [4; 32];

async fn store_node() -> Multiaddr {
    start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await
}

async fn connect(address: &Multiaddr) -> dyapp_client::Node {
    dyapp_client::Node::connect(
        address.clone(),
        std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
    )
    .await
    .unwrap()
}

async fn receive(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    device_key: &Identity,
    profiles: &[dyapp_profile::VerifiedProfile],
) -> Vec<chat::Event> {
    session
        .receive(from_mut(node), device_key, profiles, &SEAL)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_receipt_through_another_node_resends_what_the_first_one_dropped() {
    let (first, second) = (store_node().await, store_node().await);
    let mut alice = Person::new(&first).await;
    let mut bob = Person::new(&first).await;
    let (_test_dir, mut alice_session, mut bob_session, profiles, group_id) =
        open_matched_delivery(&mut alice, &mut bob).await;

    // The first node accepts "one", but Bob reads only the second from now on: it dropped it.
    let dropper = send_one(&mut alice_session, &mut alice, &group_id).await;
    let (mut alice_second, now_ms) =
        Box::pin(send_two_via_second(&mut alice_session, &group_id, &second)).await;
    let mut bob_second = connect(&second).await;
    assert_received_text(
        &mut bob_session,
        &mut bob_second,
        &bob.device_key,
        &profiles,
        &group_id,
        &alice.identity.peer_id(),
        b"two",
    )
    .await;
    assert_receipt_penalty(
        &mut alice_session,
        &mut alice_second,
        &alice.device_key,
        &profiles,
        dropper,
        now_ms,
    )
    .await;

    // The resend goes through the second node first, by score, and reaches Bob.
    Box::pin(resend_one(
        &mut alice_session,
        alice.node,
        alice_second,
        now_ms,
    ))
    .await;
    assert_received_text(
        &mut bob_session,
        &mut bob_second,
        &bob.device_key,
        &profiles,
        &group_id,
        &alice.identity.peer_id(),
        b"one",
    )
    .await;
}

async fn open_matched_delivery(
    alice: &mut Person,
    bob: &mut Person,
) -> (
    dyapp_test_support::TestDir,
    Session,
    Session,
    Vec<dyapp_profile::VerifiedProfile>,
    Vec<u8>,
) {
    let (test_dir, mut alice_session, mut bob_session, profiles, group_id) =
        open_delivery_sessions(alice, bob).await;
    match_delivery_chat(
        alice,
        bob,
        &mut alice_session,
        &mut bob_session,
        &profiles,
        &group_id,
    )
    .await;
    (test_dir, alice_session, bob_session, profiles, group_id)
}

async fn send_one(session: &mut Session, alice: &mut Person, group_id: &[u8]) -> PeerId {
    let dropper = alice.node.peer();
    session
        .send_message(from_mut(&mut alice.node), group_id, b"one", &SEAL)
        .await
        .unwrap();
    dropper
}

async fn resend_one(
    session: &mut Session,
    first: dyapp_client::Node,
    second: dyapp_client::Node,
    now_ms: i64,
) {
    let mut nodes = vec![first, second].into_boxed_slice();
    session
        .retry_due(&mut nodes, now_ms + 120_000, &SEAL)
        .await
        .unwrap();
}

async fn open_delivery_sessions(
    alice: &mut Person,
    bob: &mut Person,
) -> (
    dyapp_test_support::TestDir,
    Session,
    Session,
    Vec<dyapp_profile::VerifiedProfile>,
    Vec<u8>,
) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let group_id = group.id().to_vec();
    let mut store = Store::open(&test_dir.path().join("drop-alice.db"), &alice.identity).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let alice_session = Session::open(store, &SEAL).unwrap();
    let mut store = Store::open(&test_dir.path().join("drop-bob.db"), &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let bob_session = Session::open(store, &SEAL).unwrap();
    (test_dir, alice_session, bob_session, profiles, group_id)
}

async fn match_delivery_chat(
    alice: &mut Person,
    bob: &mut Person,
    alice_session: &mut Session,
    bob_session: &mut Session,
    profiles: &[dyapp_profile::VerifiedProfile],
    group_id: &[u8],
) {
    // Bob joins; a match opens the chat, so "two" below is no second request.
    receive(bob_session, &mut bob.node, &bob.device_key, profiles).await;
    alice_session
        .interact(from_mut(&mut alice.node), group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    bob_session
        .interact(from_mut(&mut bob.node), group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    // Both take the likes and trade tokens through the first node.
    for _ in 0..2 {
        receive(bob_session, &mut bob.node, &bob.device_key, profiles).await;
        receive(alice_session, &mut alice.node, &alice.device_key, profiles).await;
    }
}

async fn send_two_via_second(
    session: &mut Session,
    group_id: &[u8],
    second: &Multiaddr,
) -> (dyapp_client::Node, i64) {
    // Offline, "two" waits in the outbox; two minutes later the second node accepts it.
    assert!(matches!(
        session.send_message(&mut [], group_id, b"two", &SEAL).await,
        Err(Error::Client(_))
    ));
    let now_ms = super::now_ms();
    let mut second_node = connect(second).await;
    session
        .retry_due(from_mut(&mut second_node), now_ms + 120_000, &SEAL)
        .await
        .unwrap();
    (second_node, now_ms)
}

async fn assert_received_text(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    device_key: &Identity,
    profiles: &[dyapp_profile::VerifiedProfile],
    group_id: &[u8],
    sender: &str,
    expected: &[u8],
) {
    let events = receive(session, node, device_key, profiles).await;
    assert!(matches!(events.as_slice(),
        [chat::Event::Message { group, sender: actual, body: Body::Text { text, .. }, .. }]
        if group == group_id && actual == sender && text == expected));
}

async fn assert_receipt_penalty(
    session: &mut Session,
    node: &mut dyapp_client::Node,
    device_key: &Identity,
    profiles: &[dyapp_profile::VerifiedProfile],
    dropper: PeerId,
    now_ms: i64,
) {
    // Bob's receipt for "two" blames the first node, and "one" is due at once.
    // Token requests sent when the match opened may also be outstanding on that node.
    let now = u64::try_from(now_ms / 1000).unwrap();
    let before = session.scores().lock().await.penalty(dropper, now);
    let checked_items = receive(session, node, device_key, profiles).await;
    assert_eq!(checked_items.len(), 0);
    assert!(session.scores().lock().await.penalty(dropper, now) > before);
    assert!(!session.due_outbound(now_ms + 120_000).unwrap().is_empty());
}
