//! The chat calls the mobile apps make, against a real node.

use super::*;
use dyapp_ffi::{
    new_chat_state_key, save_profile_draft, ChatLikes, ChatRevocation, ChatSession, ChatState,
    IdentityKey, KeyStore, ProfileSolve, PublicProfileDraft, WrapError,
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

#[path = "ffi_chats/offline.rs"]
mod offline;
#[path = "ffi_chats/publication.rs"]
mod publication;

/// Retained credentials reopen the same native device after every session handle is dropped.
struct NativePeer {
    dir: String,
    key: Arc<IdentityKey>,
    device: Arc<IdentityKey>,
    state_key: Vec<u8>,
}

impl NativePeer {
    fn open(&self) -> Arc<ChatSession> {
        ChatSession::open(
            self.dir.clone(),
            self.key.clone(),
            self.device.clone(),
            Arc::new(TestStore),
            self.state_key.clone(),
        )
        .unwrap()
    }
}

/// An identity folder on the node's network whose device is published with `KeyPackages`.
fn retained_person(address: &Multiaddr) -> NativePeer {
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
    let state_key = new_chat_state_key(store).unwrap();
    let peer = NativePeer {
        dir,
        key,
        device: IdentityKey::generate(),
        state_key,
    };
    peer.open().publish_profile(true).unwrap();
    peer
}

fn person(address: &Multiaddr) -> (Arc<IdentityKey>, Arc<ChatSession>) {
    let peer = retained_person(address);
    (peer.key.clone(), peer.open())
}

fn receive_events(chats: &ChatSession) {
    assert!(
        chats.receive().unwrap() > 0,
        "expected a completed mailbox event"
    );
}

fn message(chats: &ChatSession, peer: &str, id: &[u8]) -> dyapp_ffi::ChatMessage {
    chats
        .chat_history(peer.to_owned())
        .unwrap()
        .messages
        .into_iter()
        .find(|row| row.id == id)
        .expect("submitted message")
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
    let likes = |ours, matched| ChatLikes {
        ours,
        matched,
        passed: false,
    };

    assert_eq!(
        alice.like(bob_id.clone(), true).unwrap(),
        likes(true, false)
    );
    receive_events(&bob);
    assert_eq!(bob.liked_you().unwrap(), std::slice::from_ref(&alice_id));
    assert_eq!(bob.likes(alice_id.clone()).unwrap(), likes(false, false));

    assert_eq!(bob.like(alice_id.clone(), true).unwrap(), likes(true, true));
    assert_eq!(bob.liked_you().unwrap(), [] as [std::string::String; 0]);
    assert_eq!(bob.state(alice_id.clone()).unwrap(), ChatState::Open);
    receive_events(&alice);
    assert_eq!(alice.likes(bob_id.clone()).unwrap(), likes(true, true));
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::Open);

    // Taking the like back ends the match and closes the chat on both sides.
    assert_eq!(
        alice.like(bob_id.clone(), false).unwrap(),
        likes(false, false)
    );
    assert_eq!(alice.state(bob_id).unwrap(), ChatState::Closed);
    receive_events(&bob);
    assert_eq!(bob.likes(alice_id.clone()).unwrap(), likes(true, false));
    assert_eq!(bob.state(alice_id).unwrap(), ChatState::Closed);
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

    receive_events(&bob);
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
    receive_events(&alice);
    assert_eq!(alice.state(bob_id.clone()).unwrap(), ChatState::Open);
    assert_eq!(bob.state(alice_id.clone()).unwrap(), ChatState::Open);
    assert_eq!(texts(&alice, &bob_id), ["hello bob", "hello alice"]);
    assert_eq!(texts(&bob, &alice_id), ["hello bob", "hello alice"]);
}

#[test]
fn an_own_text_is_edited_and_deleted_and_a_chat_cleared_and_blocked() {
    let (_runtime, address) = store_node();
    let (alice_key, alice) = person(&address);
    let (bob_key, bob) = person(&address);
    let (alice_id, bob_id) = (alice_key.peer_id(), bob_key.peer_id());
    let id = alice.send_text(bob_id.clone(), "hello bob".into()).unwrap();
    receive_events(&bob);
    assert_eq!(bob.mark_read(alice_id.clone()).unwrap(), 1);
    assert_eq!(bob.mark_read(alice_id.clone()).unwrap(), 0);
    assert_eq!(bob.chat_rows().unwrap()[0].unread_count, 0);

    let sent = message(&alice, &bob_id, &id);
    let group = sent.group_id.clone();
    let edited =
        |text: &str| alice.edit_text(bob_id.clone(), group.clone(), id.clone(), text.into());
    assert_eq!(edited("hi bob").unwrap().len(), 16);
    receive_events(&bob);
    let current = message(&bob, &alice_id, &id);
    assert_eq!(
        (current.text.as_deref(), current.edited),
        (Some("hi bob"), true)
    );
    // Only the author edits a text.
    let theirs = bob.edit_text(alice_id.clone(), group.clone(), id.clone(), "no".into());
    assert_eq!(theirs.unwrap_err().to_string(), "unknownMessage");

    alice
        .delete_text(bob_id.clone(), group.clone(), id.clone())
        .unwrap();
    receive_events(&bob);
    assert!(message(&bob, &alice_id, &id).deleted);
    assert_eq!(edited("again").unwrap_err().to_string(), "unknownMessage");

    // Clearing is local: the author keeps the deletion marker.
    assert!(bob.clear_chat(alice_id.clone()).unwrap() > 0);
    assert_eq!(
        bob.chat_history(alice_id.clone()).unwrap().messages,
        [] as [dyapp_ffi::ChatMessage; 0]
    );
    assert_eq!(
        alice.chat_history(bob_id.clone()).unwrap().messages.len(),
        1
    );

    // The node is up, so the revocation is confirmed or was not needed.
    let blocked = bob.block(alice_id.clone()).unwrap();
    assert_ne!(
        blocked.revocation,
        ChatRevocation::Pending,
        "{:?}",
        blocked.detail
    );
    assert_eq!(bob.chat_rows().unwrap(), [] as [dyapp_ffi::ChatRow; 0]);
    bob.unblock(alice_id.clone()).unwrap();
    assert_eq!(bob.chat_rows().unwrap().len(), 1);
}
