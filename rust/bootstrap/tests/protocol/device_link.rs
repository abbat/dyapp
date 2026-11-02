//! A device linked to an account joins the owner's chats, and a contact holding the older profile
//! catches up instead of falling out of the group.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use std::slice::from_mut;

const SEAL: [u8; 32] = [6; 32];

fn session(person: &Person, name: &str) -> Session {
    let path = store_path(name);
    let mut store = Store::open(&path, &person.identity).unwrap();
    store
        .stage_outbound(&person.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    Session::open(store, &SEAL).unwrap()
}

#[tokio::test]
async fn a_linked_device_joins_a_chat_and_a_stale_contact_catches_up() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let owner = alice.identity.peer_id();
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut held = Vec::new();
    for peer in [&owner, &bob_owner] {
        held.push(alice.node.get_profile(peer).await.unwrap());
    }

    // The chat begins with one device on each side.
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &held)
        .await
        .unwrap();
    let group_id = group.id().to_vec();
    let welcome = welcome
        .into_iter()
        .map(|delivery| dyapp_client::prepare_put(&delivery.mailbox, delivery.mls).unwrap())
        .collect::<Vec<_>>();
    let alice_path = store_path("link-alice");
    Store::open(&alice_path, &alice.identity)
        .unwrap()
        .stage_group(
            &group,
            &held[1],
            &alice.device.seal(&SEAL).unwrap(),
            &welcome,
        )
        .unwrap();
    let mut alice_session =
        Session::open(Store::open(&alice_path, &alice.identity).unwrap(), &SEAL).unwrap();
    let mut bob_session = session(&bob, "link-bob");
    alice.node.put_prepared(&welcome[0]).await.unwrap();
    bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &held, &SEAL)
        .await
        .unwrap();

    // The owner links a laptop: it publishes a profile that lists it, then its KeyPackages.
    let identity = Identity::from_secret(&alice.identity.secret());
    let mut laptop = Person::device(&address, identity, 2).await;
    let mut devices =
        [&alice, &laptop].map(|person| (person.mailbox(), person.device.certificate()));
    devices.sort_by_key(|(mailbox, _)| *mailbox);
    let profile = Profile {
        version: 2,
        devices: devices.map(|(_, certificate)| certificate.clone()).to_vec(),
        ..Default::default()
    };
    laptop
        .node
        .publish_profile(profile.sign(&laptop.identity))
        .await
        .unwrap();
    chat::publish_key_packages(
        &mut laptop.node,
        &laptop.device_key,
        &laptop.device,
        &owner,
        1,
    )
    .await
    .unwrap();
    let mut laptop_session = session(&laptop, "link-laptop");
    let linked = vec![
        alice.node.get_profile(&owner).await.unwrap(),
        held[1].clone(),
    ];

    let now = chrono::Utc::now().timestamp().unsigned_abs();
    for added in [1, 0] {
        assert_eq!(
            alice_session
                .add_own_devices(from_mut(&mut alice.node), &linked[0], &linked, now, &SEAL)
                .await
                .unwrap(),
            added
        );
    }

    // Bob still holds the older profile: the commit waits instead of being refused.
    let events = bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &held, &SEAL)
        .await
        .unwrap();
    assert!(matches!(
        events.as_slice(),
        [Event::UnknownOwner(peer)] if *peer == owner
    ));
    let events = bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &linked, &SEAL)
        .await
        .unwrap();
    assert!(matches!(
        events.as_slice(),
        [Event::Commit(id)] if *id == group_id
    ));

    let events = laptop_session
        .receive(
            from_mut(&mut laptop.node),
            &laptop.device_key,
            &linked,
            &SEAL,
        )
        .await
        .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Joined(id)] if *id == group_id),
        "{events:?}"
    );

    bob_session
        .send_message(from_mut(&mut bob.node), &group_id, b"to both", &SEAL)
        .await
        .unwrap();
    for (session, node, key) in [
        (&mut alice_session, &mut alice.node, &alice.device_key),
        (&mut laptop_session, &mut laptop.node, &laptop.device_key),
    ] {
        let events = session
            .receive(from_mut(node), key, &linked, &SEAL)
            .await
            .unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Message { body: Body::Text { text, .. }, .. } if text == b"to both"
        )));
    }
}
