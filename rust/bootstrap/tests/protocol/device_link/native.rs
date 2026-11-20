//! A linked native receiver obtains current authority through the real mailbox path.
use super::super::ffi_chats::{seed_row, TestStore};
use super::*;
use dyapp_ffi::{ChatSession, IdentityKey, KeyStore};
use dyapp_identity::network::{self, Network, Settings};
use std::sync::Arc;

fn native_device(person: &Person, address: &Multiaddr) -> Arc<ChatSession> {
    let dir = protocol_build_path("native-linked-device");
    std::fs::create_dir_all(&dir).unwrap();
    network::save(
        &dir,
        Settings::Active,
        &Network {
            seeds: vec![seed_row(address)],
            name: String::new(),
        },
    )
    .unwrap();
    let mut store = Store::open(&dir.join("client.db"), &person.identity).unwrap();
    store
        .stage_outbound(&person.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    drop(store);
    let key = |identity: &Identity| {
        IdentityKey::unwrap(
            Arc::new(TestStore),
            TestStore.wrap(identity.secret().to_vec()).unwrap(),
        )
        .unwrap()
    };
    ChatSession::open(
        dir.to_str().unwrap().into(),
        key(&person.identity),
        key(&person.device_key),
        Arc::new(TestStore),
        TestStore.wrap(SEAL.to_vec()).unwrap(),
    )
    .unwrap()
}

struct ExistingChat {
    address: Multiaddr,
    alice: Person,
    bob: Person,
    sender: Session,
    contact: Session,
    group: Vec<u8>,
    held: Vec<dyapp_profile::VerifiedProfile>,
}

impl ExistingChat {
    async fn new() -> Self {
        let address = link_node().await;
        let mut alice = Person::new(&address).await;
        let mut bob = Person::new(&address).await;
        let owner = alice.identity.peer_id();
        let peer = bob.identity.peer_id();
        chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &peer, 1)
            .await
            .unwrap();
        let held = vec![
            alice.node.get_profile(&owner).await.unwrap(),
            alice.node.get_profile(&peer).await.unwrap(),
        ];
        let (mut sender, contact, group) = begin_chat(&mut alice, &mut bob, &held).await;
        sender
            .send_message(
                from_mut(&mut alice.node),
                &group,
                b"before enrollment",
                &SEAL,
            )
            .await
            .unwrap();
        Self {
            address,
            alice,
            bob,
            sender,
            contact,
            group,
            held,
        }
    }

    async fn link(&mut self) -> Person {
        let (mut laptop, unused, linked) =
            Box::pin(publish_laptop(&mut self.alice, &self.address, &self.held)).await;
        drop(unused);
        chat::publish_key_packages(
            &mut laptop.node,
            &laptop.device_key,
            &laptop.device,
            &self.alice.identity.peer_id(),
            4,
        )
        .await
        .unwrap();
        self.sender
            .maintain_own_devices(
                from_mut(&mut self.alice.node),
                &linked[0],
                &linked,
                1_000_000,
                &SEAL,
            )
            .await
            .unwrap();
        self.held = linked;
        laptop
    }

    async fn deliver_new_message(&mut self) {
        self.contact
            .receive(
                from_mut(&mut self.bob.node),
                &self.bob.device_key,
                &self.held,
                &SEAL,
            )
            .await
            .unwrap();
        self.contact
            .send_message(
                from_mut(&mut self.bob.node),
                &self.group,
                b"after enrollment",
                &SEAL,
            )
            .await
            .unwrap();
        self.sender
            .receive(
                from_mut(&mut self.alice.node),
                &self.alice.device_key,
                &self.held,
                &SEAL,
            )
            .await
            .unwrap();
    }
}

#[test]
fn native_linked_receiver_joins_existing_chat_without_old_history() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut chat = runtime.block_on(ExistingChat::new());
    let laptop = runtime.block_on(chat.link());
    let receiver = native_device(&laptop, &chat.address);
    assert!(receiver.receive().unwrap() > 0);
    let peer = chat.bob.identity.peer_id();
    assert!(receiver
        .chat_history(peer.clone())
        .unwrap()
        .messages
        .is_empty());
    runtime.block_on(chat.deliver_new_message());
    assert!(receiver.receive().unwrap() > 0);
    let messages = receiver.chat_history(peer).unwrap().messages;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text.as_deref(), Some("after enrollment"));
}
