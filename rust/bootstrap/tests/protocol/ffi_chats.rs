//! The chat calls the mobile apps make, against a real node.

use super::*;
use dyapp_ffi::{
    new_chat_state_key, save_profile_draft, ChatLikes, ChatSession, ChatState, IdentityKey,
    KeyStore, ProfileSolve, PublicProfileDraft, WrapError,
};
use dyapp_identity::network::{self, Network, Settings};
use libp2p::multiaddr::Protocol;
use std::sync::Arc;

/// XOR with a fixed byte: stands in for the platform keystore.
struct TestStore;

impl KeyStore for TestStore {
    fn wrap(&self, secret: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        Ok(secret.iter().map(|byte| byte ^ 0x5a).collect())
    }

    fn unwrap(&self, wrapped: Vec<u8>) -> Result<Vec<u8>, WrapError> {
        self.wrap(wrapped)
    }
}

/// The `host:port/<peer ID>` row of a node's loopback TCP address.
fn seed_row(address: &Multiaddr) -> String {
    let (mut port, mut peer) = (None, None);
    for part in address {
        match part {
            Protocol::Tcp(found) => port = Some(found),
            Protocol::P2p(found) => peer = Some(found),
            _ => {}
        }
    }
    format!("127.0.0.1:{}/{}", port.unwrap(), peer.unwrap())
}

/// An identity folder on the node's network whose device is published with KeyPackages.
fn person(address: &Multiaddr) -> (Arc<IdentityKey>, Arc<ChatSession>) {
    let dir = protocol_build_path("ffi-chats");
    std::fs::create_dir_all(&dir).unwrap();
    let active = Network {
        seeds: vec![seed_row(address)],
        name: String::new(),
    };
    network::save(&dir, Settings::Active, &active).unwrap();
    let dir = dir.to_str().unwrap().to_owned();
    let key = IdentityKey::generate();
    save_profile_draft(
        dir.clone(),
        key.clone(),
        PublicProfileDraft {
            gender: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let solve = ProfileSolve::new(dir.clone(), key.clone()).unwrap();
    while !solve.run_step(true).unwrap().solved {}
    let store: Arc<dyn KeyStore> = Arc::new(TestStore);
    let state_key = new_chat_state_key(store.clone()).unwrap();
    let chats =
        ChatSession::open(dir, key.clone(), IdentityKey::generate(), store, state_key).unwrap();
    chats.publish_profile(true).unwrap();
    (key, chats)
}

fn texts(chats: &ChatSession, peer_id: &str) -> Vec<String> {
    chats
        .chat_history(peer_id.to_owned())
        .unwrap()
        .messages
        .into_iter()
        .map(|message| message.text.unwrap())
        .collect()
}

/// A store node; the sessions block on their own runtimes, so it needs threads of its own.
fn store_node() -> (tokio::runtime::Runtime, Multiaddr) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let address = runtime.block_on(start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    }));
    (runtime, address)
}

#[test]
fn a_like_reaches_the_peer_and_a_mutual_like_opens_the_chat() {
    let (_runtime, address) = store_node();
    let (alice_key, alice) = person(&address);
    let (bob_key, bob) = person(&address);
    let (alice_id, bob_id) = (alice_key.peer_id(), bob_key.peer_id());
    let likes = |ours, matched| ChatLikes { ours, matched };

    assert_eq!(
        alice.like(bob_id.clone(), true).unwrap(),
        likes(true, false)
    );
    assert!(bob.receive().unwrap() > 0);
    assert_eq!(bob.liked_you().unwrap(), [alice_id.clone()]);
    assert_eq!(bob.likes(alice_id.clone()).unwrap(), likes(false, false));

    assert_eq!(bob.like(alice_id.clone(), true).unwrap(), likes(true, true));
    assert!(bob.liked_you().unwrap().is_empty());
    assert_eq!(bob.state(alice_id.clone()).unwrap(), ChatState::Open);
    assert!(alice.receive().unwrap() > 0);
    assert_eq!(alice.likes(bob_id.clone()).unwrap(), likes(true, true));
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::Open);

    // Taking the like back ends the match but the chat stays open.
    assert_eq!(
        alice.like(bob_id.clone(), false).unwrap(),
        likes(false, false)
    );
    assert_eq!(alice.state(bob_id).unwrap(), ChatState::Open);
    assert!(bob.receive().unwrap() > 0);
    assert_eq!(bob.likes(alice_id.clone()).unwrap(), likes(true, false));
    assert_eq!(bob.state(alice_id).unwrap(), ChatState::Open);
}

#[test]
fn two_chat_sessions_exchange_a_text_and_list_it() {
    let (_runtime, address) = store_node();
    let (alice_key, alice) = person(&address);
    let (bob_key, bob) = person(&address);
    let (alice_id, bob_id) = (alice_key.peer_id(), bob_key.peer_id());

    alice.send_text(bob_id.clone(), "hello bob".into()).unwrap();
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::RequestSent);
    assert_eq!(texts(&alice, &bob_id), ["hello bob"]);

    assert!(bob.receive().unwrap() > 0);
    assert_eq!(
        bob.state(alice_id.clone()).unwrap(),
        ChatState::RequestReceived
    );
    let rows = bob.chat_rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].peer_id, alice_id);
    assert_eq!(rows[0].unread_count, 1);
    let last = rows[0].last_message.as_ref().unwrap();
    assert_eq!(last.author, alice_id);
    assert_eq!(last.text.as_deref(), Some("hello bob"));
    assert_eq!(last.delivery, None);

    bob.send_text(alice_id.clone(), "hello alice".into())
        .unwrap();
    assert!(alice.receive().unwrap() > 0);
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::Open);
    assert_eq!(bob.state(alice_id.clone()).unwrap(), ChatState::Open);
    assert_eq!(texts(&alice, &bob_id), ["hello bob", "hello alice"]);
    assert_eq!(texts(&bob, &alice_id), ["hello bob", "hello alice"]);
}
