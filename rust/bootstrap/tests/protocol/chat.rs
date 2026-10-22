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
        let mut person = Self::device(address, Identity::generate(), 1).await;
        let profile = Profile {
            version: 1,
            devices: vec![person.device.certificate().clone()],
            ..Default::default()
        };
        person
            .node
            .publish_profile(profile.sign(&person.identity))
            .await
            .unwrap();
        person
    }

    /// A device `generation` of `identity`, not yet in any published profile.
    async fn device(address: &Multiaddr, identity: Identity, generation: u64) -> Self {
        let device_key = Identity::generate();
        let device = Device::new(&identity, device_key.public_key(), generation).unwrap();
        let node = dyapp_client::Node::connect(
            address.clone(),
            std::sync::Arc::new(dyapp_p2p_net::Protocols::public()),
        )
        .await
        .unwrap();
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

#[tokio::test]
async fn a_revoked_device_loses_its_mailbox_and_new_messages() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let identity = Identity::from_secret(&alice.identity.secret());
    let mut lost = Person::device(&address, identity, 2).await;
    // Signed when the second device is linked, and kept by the first.
    let binding = dyapp_client::owner_binding(&lost.device_key, &alice.identity.public_key());
    // A profile lists its devices sorted by mailbox address.
    let mut devices = [&alice, &lost].map(|person| (person.mailbox(), person.device.certificate()));
    devices.sort_by_key(|(mailbox, _)| *mailbox);
    let linked = Profile {
        version: 2,
        devices: devices.map(|(_, certificate)| certificate.clone()).to_vec(),
        ..Default::default()
    };
    alice
        .node
        .publish_profile(linked.sign(&alice.identity))
        .await
        .unwrap();
    let mut bob = Person::new(&address).await;
    for person in [&mut lost, &mut bob] {
        let owner = person.identity.peer_id();
        chat::publish_key_packages(
            &mut person.node,
            &person.device_key,
            &person.device,
            &owner,
            1,
        )
        .await
        .unwrap();
    }
    let mut profiles = vec![
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
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    assert_eq!(welcome.len(), 2);
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let mut lost_groups = Vec::new();
    chat::receive(
        &mut lost.node,
        &lost.device_key,
        &lost.device,
        &mut lost_groups,
        &profiles,
    )
    .await
    .unwrap();
    let mut bob_groups = Vec::new();
    let (_, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut bob_groups,
        &profiles,
    )
    .await
    .unwrap();
    bob.node.ack(&bob.device_key, ids).await.unwrap();
    assert_eq!(lost_groups.len(), 1);
    assert_eq!(bob_groups.len(), 1);

    // The owner drops the lost device and closes its mailbox from the first device.
    let dropped = Profile {
        version: 3,
        devices: vec![alice.device.certificate().clone()],
        ..Default::default()
    };
    alice
        .node
        .publish_profile(dropped.sign(&alice.identity))
        .await
        .unwrap();
    alice
        .node
        .close_mailbox(&alice.identity, binding)
        .await
        .unwrap();
    profiles[0] = bob
        .node
        .get_profile(&alice.identity.peer_id())
        .await
        .unwrap();
    assert!(chat::revoked(&lost.device, &profiles[0]));
    assert!(!chat::revoked(&alice.device, &profiles[0]));

    // Bob, a contact, sees the newer profile and removes the device from the group.
    let commit = bob_groups[0]
        .remove_revoked(&bob.device, &profiles)
        .unwrap()
        .unwrap();
    let deliveries = chat::to_members(&bob_groups[0], &commit).unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].mailbox, alice.mailbox());
    chat::send(&mut bob.node, &deliveries).await.unwrap();
    let secret = b"after the revocation";
    let sent = chat::message(&mut bob_groups[0], &bob.device, secret).unwrap();
    assert_eq!(sent.len(), 1);
    chat::send(&mut bob.node, &sent).await.unwrap();

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
        events.last(),
        Some(&Event::Message {
            group: alice_groups[0].id().to_vec(),
            sender: bob.identity.peer_id(),
            plaintext: secret.to_vec(),
        })
    );
    // The lost device can neither fetch its mailbox nor read the new epoch's message.
    assert!(matches!(
        lost.node.fetch(&lost.device_key).await,
        Err(dyapp_client::Error::Status(Status::Refused))
    ));
    assert!(lost_groups[0]
        .process(&lost.device, &sent[0].mls, &profiles)
        .is_err());
}
