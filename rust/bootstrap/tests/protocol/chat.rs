use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::profiles;
use dyapp_client::store::Store;
use dyapp_messaging::mls::{Device, Group};
use dyapp_profile::VerifiedProfile;

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
    let sent = chat::message(&mut group, &alice.device, &Body::Text(hello.to_vec())).unwrap();
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
                body: Body::Text(hello.to_vec()),
            },
        ]
    );
    bob.node.ack(&bob.device_key, ids).await.unwrap();
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());

    let reply = chat::message(
        &mut bob_groups[0],
        &bob.device,
        &Body::Text(b"hi alice".to_vec()),
    )
    .unwrap();
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
            body: Body::Text(b"hi alice".to_vec()),
        }]
    );
}

#[tokio::test]
async fn a_welcome_waits_in_the_mailbox_for_its_sender_s_profile() {
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
    // Bob holds only his own profile, so Alice's membership cannot be checked yet.
    let mut groups = Vec::new();
    let (events, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut groups,
        &profiles[1..],
    )
    .await
    .unwrap();
    assert_eq!(events, vec![Event::UnknownOwner(alice.identity.peer_id())]);
    assert!(ids.is_empty());
    let (events, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut groups,
        &profiles,
    )
    .await
    .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Joined(_)]),
        "{events:?}"
    );
    assert_eq!(ids.len(), 1);
}

#[tokio::test]
async fn the_client_takes_the_newest_replica_profile_and_refuses_a_rollback() {
    let mut nodes = Vec::new();
    for _ in 0..2 {
        let address = start_config(|config| {
            config.roles = vec![Role::Store];
            config.limits.requests_per_second = 1000;
            config.limits.min_free_mb = 0;
        })
        .await;
        nodes.push(Person::device(&address, Identity::generate(), 1).await.node);
    }
    let owner = Identity::generate();
    for (node, version) in nodes.iter_mut().zip([3, 5]) {
        let profile = Profile {
            version,
            ..Default::default()
        };
        node.publish_profile(profile.sign(&owner)).await.unwrap();
    }

    let mut scores = dyapp_client::scores::Scores::default();
    let newest = profiles::newest(&mut scores, &mut nodes, &owner.peer_id())
        .await
        .unwrap();
    assert_eq!(newest.profile.version, 5);
    // The node that served version 3 loses score.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(scores.penalty(nodes[0].peer(), now), 1);
    assert_eq!(scores.penalty(nodes[1].peer(), now), 0);
    let mut held = profiles::Profiles::default();
    held.accept(newest).unwrap();
    let older = nodes[0].get_profile(&owner.peer_id()).await.unwrap();
    assert_eq!(older.profile.version, 3);
    assert!(matches!(
        held.accept(older),
        Err(dyapp_client::Error::Rollback)
    ));
    assert_eq!(held.get(&owner.peer_id()).unwrap().profile.version, 5);
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
    let sent = chat::message(
        &mut bob_groups[0],
        &bob.device,
        &Body::Text(secret.to_vec()),
    )
    .unwrap();
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
            body: Body::Text(secret.to_vec()),
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

#[tokio::test]
async fn a_hidden_commit_shows_as_a_gap_and_the_second_replica_delivers_it() {
    let mut addresses = Vec::new();
    for _ in 0..2 {
        addresses.push(
            start_config(|config| {
                config.roles = vec![Role::Store];
                config.limits.requests_per_second = 1000;
                config.limits.min_free_mb = 0;
            })
            .await,
        );
    }
    let mut alice = Person::new(&addresses[0]).await;
    let mut bob = Person::new(&addresses[0]).await;
    let mut carol = Person::new(&addresses[0]).await;
    for person in [&mut bob, &mut carol] {
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
    let mut profiles = Vec::new();
    for owner in [&alice, &bob, &carol].map(|person| person.identity.peer_id()) {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (mut group, welcome) =
        chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
            .await
            .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
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
    assert_eq!(events, vec![Event::Joined(group.id().to_vec())]);
    bob.node.ack(&bob.device_key, ids).await.unwrap();

    // Alice drops Carol; the first node hides the commit, which only the second one holds.
    let commit = group
        .remove_revoked(&alice.device, &profiles[..2])
        .unwrap()
        .unwrap();
    let mut second = Person::device(&addresses[1], Identity::generate(), 1)
        .await
        .node;
    chat::send(&mut second, &chat::to_members(&group, &commit).unwrap())
        .await
        .unwrap();
    let sent = chat::message(
        &mut group,
        &alice.device,
        &Body::Text(b"carol is gone".to_vec()),
    )
    .unwrap();
    chat::send(&mut alice.node, &sent).await.unwrap();

    let (events, ids) = chat::receive(
        &mut bob.node,
        &bob.device_key,
        &bob.device,
        &mut bob_groups,
        &profiles,
    )
    .await
    .unwrap();
    assert_eq!(events, vec![Event::Gap(group.id().to_vec())]);
    assert!(ids.is_empty());

    let mut scores = dyapp_client::scores::Scores::default();
    let (events, ids) = chat::cross_check(
        &mut scores,
        &mut bob.node,
        &mut second,
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
            Event::Commit(group.id().to_vec()),
            Event::Message {
                group: group.id().to_vec(),
                sender: alice.identity.peer_id(),
                body: Body::Text(b"carol is gone".to_vec()),
            },
        ]
    );
    assert_eq!(bob_groups[0].members().unwrap().len(), 2);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(scores.penalty(bob.node.peer(), now), 1);
    assert_eq!(scores.penalty(second.peer(), now), 0);
    bob.node.ack(&bob.device_key, ids.clone()).await.unwrap();
    second.ack(&bob.device_key, ids).await.unwrap();
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());
    assert!(second.fetch(&bob.device_key).await.unwrap().0.is_empty());
}

/// A fresh database path for one person's client store.
fn store_path(name: &str) -> std::path::PathBuf {
    let mut random = [0; 16];
    getrandom::fill(&mut random).unwrap();
    std::env::temp_dir().join(format!(
        "dyapp-{name}-{:032x}.sqlite3",
        u128::from_be_bytes(random)
    ))
}

/// Make every MLS state write in the database at `path` fail until `heal`.
fn break_mls_writes(path: &std::path::Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END",
        )
        .unwrap();
}

fn heal(path: &std::path::Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch("DROP TRIGGER `fail_mls`")
        .unwrap();
}

#[tokio::test]
async fn durable_chat_survives_restarts_storage_faults_and_retries() {
    use dyapp_client::session::{Error, Session};

    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let (alice_path, bob_path) = (store_path("alice"), store_path("bob"));
    let (alice_key, bob_key) = ([1; 32], [2; 32]);
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    Store::open(&bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&bob_key).unwrap(), &[])
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner.clone()] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }

    // Alice commits the new group and its Welcome, then stops before any put.
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    let group_id = group.id().to_vec();
    let writes = welcome
        .into_iter()
        .map(|delivery| dyapp_client::prepare_put(&delivery.mailbox, delivery.mls).unwrap())
        .collect::<Vec<_>>();
    Store::open(&alice_path, &alice.identity)
        .unwrap()
        .stage_group(
            &group,
            &profiles[1],
            &alice.device.seal(&alice_key).unwrap(),
            &writes,
        )
        .unwrap();
    drop(group);
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());

    // After a restart the exact signed Welcome is retried from the outbox.
    let mut alice_session = Session::open(
        Store::open(&alice_path, &alice.identity).unwrap(),
        &alice_key,
    )
    .unwrap();
    let due = alice_session.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].as_bytes(), writes[0].as_bytes());
    alice.node.put_prepared(&due[0]).await.unwrap();

    // A failed local commit sends nothing and needs the saved snapshot reopened.
    break_mls_writes(&alice_path);
    assert!(matches!(
        alice_session
            .send_message(&mut alice.node, &group_id, b"never sent", &alice_key)
            .await,
        Err(Error::Store(_))
    ));
    assert!(matches!(
        alice_session
            .send_message(&mut alice.node, &group_id, b"never sent", &alice_key)
            .await,
        Err(Error::Invalidated)
    ));
    heal(&alice_path);
    alice_session.recover(&alice_key).unwrap();
    alice_session
        .send_message(&mut alice.node, &group_id, b"hello bob", &alice_key)
        .await
        .unwrap();
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 2);

    // Bob's failed receive commit shows and acknowledges nothing.
    let mut bob_session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &bob_key).unwrap();
    break_mls_writes(&bob_path);
    assert!(matches!(
        bob_session
            .receive(&mut bob.node, &bob.device_key, &profiles, &bob_key)
            .await,
        Err(Error::Store(_))
    ));
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 2);
    heal(&bob_path);
    drop(bob_session);

    let hello = Event::Message {
        group: group_id.clone(),
        sender: alice.identity.peer_id(),
        body: Body::Text(b"hello bob".to_vec()),
    };
    let mut bob_session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &bob_key).unwrap();
    assert_eq!(
        bob_session
            .receive(&mut bob.node, &bob.device_key, &profiles, &bob_key)
            .await
            .unwrap(),
        [Event::Joined(group_id.clone()), hello]
    );
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());

    // Alice retries every saved write with its exact bytes; Bob shows nothing twice.
    drop(alice_session);
    let mut alice_session = Session::open(
        Store::open(&alice_path, &alice.identity).unwrap(),
        &alice_key,
    )
    .unwrap();
    let due = alice_session.due_outbound(i64::MAX).unwrap();
    assert_eq!(due.len(), 2);
    for write in &due {
        alice.node.put_prepared(write).await.unwrap();
    }
    drop(bob_session);
    let mut bob_session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &bob_key).unwrap();
    assert!(bob_session
        .receive(&mut bob.node, &bob.device_key, &profiles, &bob_key)
        .await
        .unwrap()
        .is_empty());
    assert!(bob.node.fetch(&bob.device_key).await.unwrap().0.is_empty());

    // The restored group keeps working in both directions.
    bob_session
        .send_message(&mut bob.node, &group_id, b"hi alice", &bob_key)
        .await
        .unwrap();
    assert_eq!(
        alice_session
            .receive(&mut alice.node, &alice.device_key, &profiles, &alice_key)
            .await
            .unwrap(),
        [Event::Message {
            group: group_id,
            sender: bob_owner,
            body: Body::Text(b"hi alice".to_vec()),
        }]
    );
}

async fn post(person: &mut Person, group: &mut Group, bodies: &[Body]) {
    for body in bodies {
        let deliveries = chat::message(group, &person.device, body).unwrap();
        chat::send(&mut person.node, &deliveries).await.unwrap();
    }
}

async fn fetch(
    person: &mut Person,
    groups: &mut Vec<Group>,
    profiles: &[VerifiedProfile],
) -> Vec<Event> {
    let (events, ids) = chat::receive(
        &mut person.node,
        &person.device_key,
        &person.device,
        groups,
        profiles,
    )
    .await
    .unwrap();
    person.node.ack(&person.device_key, ids).await.unwrap();
    events
}

/// The token request for every other device in `group`, if one is due on `day`.
fn ask(store: &mut Store, group: &Group, day: u32, seal: &[u8; 32]) -> Vec<Body> {
    let mailboxes = group
        .peer_device_keys()
        .unwrap()
        .iter()
        .map(|key| dyapp_client::mailbox(key.as_slice().try_into().unwrap()))
        .collect::<Vec<_>>();
    dyapp_client::tokens::request(store, &mailboxes, day, seal)
        .unwrap()
        .into_iter()
        .collect()
}

/// Answers the token requests among `events` and stores the token answers; returns the answers
/// to send and how many tokens it stored.
fn trade(
    person: &Person,
    store: &Store,
    events: &[Event],
    day: u32,
    seal: &[u8; 32],
) -> (Vec<Body>, usize) {
    use dyapp_client::tokens;
    let (mut answers, mut stored) = (Vec::new(), 0);
    for event in events {
        match event {
            Event::Message {
                sender,
                body: Body::TokenRequest(request),
                ..
            } => answers
                .extend(tokens::answer(store, &person.device_key, sender, request, day).unwrap()),
            Event::Message {
                body: Body::TokenAnswer(answer),
                ..
            } => stored += tokens::accept(store, answer, seal).unwrap(),
            _ => {}
        }
    }
    (answers, stored)
}

#[tokio::test]
async fn contacts_trade_a_month_of_write_tokens_after_join_and_refill_them() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let mut alice_store = Store::open(&store_path("alice"), &alice.identity).unwrap();
    let mut bob_store = Store::open(&store_path("bob"), &bob.identity).unwrap();
    let (alice_seal, bob_seal) = ([1; 32], [2; 32]);

    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let mut alice_groups = vec![group];
    let mut bob_groups = Vec::new();
    assert_eq!(
        fetch(&mut bob, &mut bob_groups, &profiles).await,
        [Event::Joined(alice_groups[0].id().to_vec())]
    );

    // After the join each side asks the other for a month; a refill at 14 days left tops it up.
    let today = 20_000;
    for (day, expected) in [(today, 30), (today + 16, 16)] {
        let asked = ask(&mut alice_store, &alice_groups[0], day, &alice_seal);
        post(&mut alice, &mut alice_groups[0], &asked).await;
        let asked = ask(&mut bob_store, &bob_groups[0], day, &bob_seal);
        post(&mut bob, &mut bob_groups[0], &asked).await;

        let events = fetch(&mut bob, &mut bob_groups, &profiles).await;
        let (answers, _) = trade(&bob, &bob_store, &events, day, &bob_seal);
        post(&mut bob, &mut bob_groups[0], &answers).await;
        let events = fetch(&mut alice, &mut alice_groups, &profiles).await;
        let (answers, stored) = trade(&alice, &alice_store, &events, day, &alice_seal);
        assert_eq!(stored, expected);
        post(&mut alice, &mut alice_groups[0], &answers).await;
        let events = fetch(&mut bob, &mut bob_groups, &profiles).await;
        assert_eq!(trade(&bob, &bob_store, &events, day, &bob_seal).1, expected);

        let month = (day..day + 30).collect::<Vec<_>>();
        assert_eq!(
            alice_store.write_token_epochs(&bob.mailbox(), day).unwrap(),
            month
        );
        assert_eq!(
            bob_store.write_token_epochs(&alice.mailbox(), day).unwrap(),
            month
        );
    }
    assert!(ask(&mut alice_store, &alice_groups[0], today + 17, &alice_seal).is_empty());
}
