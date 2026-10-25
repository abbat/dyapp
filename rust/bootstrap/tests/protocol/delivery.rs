//! Resends through another node once a receipt shows the first one dropped a write.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body};
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
    let mut store = Store::open(&store_path("drop-alice"), &alice.identity).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let mut alice_session = Session::open(store, &SEAL).unwrap();
    let mut store = Store::open(&store_path("drop-bob"), &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();
    // Bob joins; both trade tokens through the first node.
    for _ in 0..2 {
        receive(&mut bob_session, &mut bob.node, &bob.device_key, &profiles).await;
        receive(
            &mut alice_session,
            &mut alice.node,
            &alice.device_key,
            &profiles,
        )
        .await;
    }

    // The first node accepts "one", but Bob reads only the second from now on: it dropped it.
    let dropper = alice.node.peer();
    alice_session
        .send_message(from_mut(&mut alice.node), &group_id, b"one", &SEAL)
        .await
        .unwrap();
    // Offline, "two" waits in the outbox; two minutes later the second node accepts it.
    assert!(matches!(
        alice_session
            .send_message(&mut [], &group_id, b"two", &SEAL)
            .await,
        Err(Error::Client(_))
    ));
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut alice_second = connect(&second).await;
    alice_session
        .retry_due(from_mut(&mut alice_second), now_ms + 120_000, &SEAL)
        .await
        .unwrap();
    let mut bob_second = connect(&second).await;
    let text = |text: &[u8]| chat::Event::Message {
        group: group_id.clone(),
        sender: alice.identity.peer_id(),
        body: Body::Text(text.to_vec()),
    };
    assert_eq!(
        receive(
            &mut bob_session,
            &mut bob_second,
            &bob.device_key,
            &profiles
        )
        .await,
        [text(b"two")]
    );

    // Bob's receipt for "two" blames the first node, and "one" is due at once.
    assert!(receive(
        &mut alice_session,
        &mut alice_second,
        &alice.device_key,
        &profiles
    )
    .await
    .is_empty());
    let now = (now_ms / 1000) as u64;
    assert_eq!(alice_session.scores().penalty(dropper, now), 1);
    assert_eq!(
        alice_session.due_outbound(now_ms + 120_000).unwrap().len(),
        1
    );

    // The resend goes through the second node first, by score, and reaches Bob.
    let mut nodes = [alice.node, alice_second];
    alice_session
        .retry_due(&mut nodes, now_ms + 120_000, &SEAL)
        .await
        .unwrap();
    assert_eq!(
        receive(
            &mut bob_session,
            &mut bob_second,
            &bob.device_key,
            &profiles
        )
        .await,
        [text(b"one")]
    );
}
