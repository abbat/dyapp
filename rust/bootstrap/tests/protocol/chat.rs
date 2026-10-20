use super::*;
use dyapp_client::chat::{self, Event};
use dyapp_messaging::mls::Device;

/// One person with one device: identity, device mailbox key, MLS state and node connection.
struct Person {
    identity: Identity,
    device_key: Identity,
    device: Device,
    node: dyapp_client::Node,
}

impl Person {
    async fn new(address: &Multiaddr) -> Self {
        let identity = Identity::generate();
        let device_key = Identity::generate();
        let device = Device::new(&identity, device_key.public_key(), 1).unwrap();
        let mut node = dyapp_client::Node::connect(
            address.clone(),
            std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
        )
        .await
        .unwrap();
        let profile = Profile {
            version: 1,
            devices: vec![device.certificate().clone()],
            ..Default::default()
        };
        node.publish_profile(profile.sign(&identity)).await.unwrap();
        Self {
            identity,
            device_key,
            device,
            node,
        }
    }

    fn mailbox(&self) -> [u8; 32] {
        dyapp_client::mailbox(&self.device_key.public_key())
    }
}

#[tokio::test]
async fn two_clients_chat_and_the_node_holds_only_mls_ciphertext() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    chat::publish_key_packages(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &bob.identity.peer_id(),
        2,
    )
    .await
    .unwrap();
    let profiles = vec![
        alice
            .node
            .get_profile(&alice.identity.peer_id())
            .await
            .unwrap(),
        alice
            .node
            .get_profile(&bob.identity.peer_id())
            .await
            .unwrap(),
    ];

    let alice_mailbox = alice.mailbox();
    let (mut group, welcome) =
        chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
            .await
            .unwrap();
    assert_eq!(welcome.len(), 1);
    assert_eq!(welcome[0].mailbox, bob.mailbox());
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let hello = b"hello bob, this text must never reach the node";
    let sent = chat::message(&mut group, &alice.device, hello).unwrap();
    chat::send(&mut alice.node, &sent).await.unwrap();

    // What the node stores and returns is MLS bytes, padded and encrypted.
    let (stored, _) = bob.node.fetch(&bob.device_key).await.unwrap();
    assert_eq!(stored.len(), 2);
    for envelope in &stored {
        assert!(!envelope
            .ciphertext
            .windows(hello.len())
            .any(|window| window == hello));
    }
    assert!(stored[1].ciphertext.len() >= chat::MIN_PADDED_BYTES);
    // Sealed sender: each envelope has its own signing key, and none of Alice's published keys.
    assert_ne!(stored[0].sender_key, stored[1].sender_key);
    let published = [
        alice.identity.public_key().to_vec(),
        alice.device_key.public_key().to_vec(),
    ];
    let certificate = alice.device.certificate().encode_to_vec();
    for envelope in &stored {
        assert_eq!(envelope.sender_key.len(), 32);
        assert!(!published.contains(&envelope.sender_key));
        assert!(!certificate
            .windows(envelope.sender_key.len())
            .any(|window| window == envelope.sender_key));
    }

    let mut bob_groups = Vec::new();
    let (events, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut bob_groups,
        &profiles,
    )
    .await
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Joined(group.id().to_vec()),
            Event::Message {
                group: group.id().to_vec(),
                sender: alice.identity.peer_id(),
                plaintext: hello.to_vec(),
            },
        ]
    );
    bob.node.ack(&bob.device_key, ids).await.unwrap();
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());

    let reply = chat::message(&mut bob_groups[0], &bob.device, b"hi alice").unwrap();
    assert_eq!(reply[0].mailbox, alice.mailbox());
    chat::send(&mut bob.node, &reply).await.unwrap();
    let mut alice_groups = vec![group];
    let (events, _) = chat::receive(
        &mut alice.node,
        &alice.device_key,
        &alice.device,
        &mut alice_groups,
        &profiles,
    )
    .await
    .unwrap();
    assert_eq!(
        events,
        vec![Event::Message {
            group: alice_groups[0].id().to_vec(),
            sender: bob.identity.peer_id(),
            plaintext: b"hi alice".to_vec(),
        }]
    );
}

#[tokio::test]
async fn a_stranger_s_welcome_is_refused_and_acked() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    chat::publish_key_packages(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &bob.identity.peer_id(),
        1,
    )
    .await
    .unwrap();
    let profiles = vec![
        alice
            .node
            .get_profile(&alice.identity.peer_id())
            .await
            .unwrap(),
        alice
            .node
            .get_profile(&bob.identity.peer_id())
            .await
            .unwrap(),
    ];
    let alice_mailbox = alice.mailbox();
    let (_, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    // Bob holds only his own profile, so Alice's membership cannot be checked.
    let (events, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut Vec::new(),
        &profiles[1..],
    )
    .await
    .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Refused(_)]),
        "{events:?}"
    );
    assert_eq!(ids.len(), 1);
}
