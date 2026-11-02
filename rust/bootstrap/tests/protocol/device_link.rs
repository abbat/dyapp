//! A device linked to an account joins the owner's chats, and a contact holding the older profile
//! catches up instead of falling out of the group.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::interactions::Kind;
use dyapp_client::link::{Code, Invite};
use dyapp_client::publish::{self, Failure};
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
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

    // A like and an unlike from the phone hold on the laptop too, with the chat and its copy.
    bob_session
        .interact(from_mut(&mut bob.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    laptop_session
        .receive(
            from_mut(&mut laptop.node),
            &laptop.device_key,
            &linked,
            &SEAL,
        )
        .await
        .unwrap();
    assert!(laptop_session.likes(&bob_owner).unwrap().matched());
    assert_eq!(
        laptop_session.conversation(&bob_owner).unwrap(),
        Conversation::Open
    );
    alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::Unlike, &SEAL)
        .await
        .unwrap();
    laptop_session
        .receive(
            from_mut(&mut laptop.node),
            &laptop.device_key,
            &linked,
            &SEAL,
        )
        .await
        .unwrap();
    assert!(!laptop_session.likes(&bob_owner).unwrap().ours);
    assert_eq!(
        laptop_session.conversation(&bob_owner).unwrap(),
        Conversation::Closed
    );
    assert!(laptop_session.store().chat_rows().unwrap().is_empty());
}

#[tokio::test]
async fn a_device_that_missed_an_edit_never_overwrites_it() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let owner = alice.identity.peer_id();
    let held = alice.node.get_profile(&owner).await.unwrap().profile;
    let laptop = Store::open(&store_path("publish-laptop"), &alice.identity).unwrap();
    let phone = Store::open(&store_path("publish-phone"), &alice.identity).unwrap();

    let edit = |year, base: &Profile| Profile {
        birth_year: Some(year),
        ..base.clone()
    };
    let edited = publish::publish(
        &laptop,
        &mut alice.node,
        &alice.identity,
        &edit(1990, &held),
    )
    .await
    .unwrap();

    // The phone never saw that edit: it takes the newer profile and the node keeps it.
    let failure = publish::publish(&phone, &mut alice.node, &alice.identity, &edit(1991, &held))
        .await
        .unwrap_err();
    assert!(matches!(failure, Failure::Missed), "{failure}");
    assert_eq!(phone.profile_draft().unwrap().unwrap(), edited);
    assert_eq!(
        alice.node.get_profile(&owner).await.unwrap().profile,
        edited
    );

    // Publishing from what it took keeps the device list, and a lost reply is not a conflict.
    let draft = edit(1991, &edited);
    let next = publish::publish(&phone, &mut alice.node, &alice.identity, &draft)
        .await
        .unwrap();
    assert_eq!(next.version, edited.version + 1);
    assert_eq!(next.devices, held.devices);
    assert_eq!(
        publish::publish(&phone, &mut alice.node, &alice.identity, &draft)
            .await
            .unwrap(),
        next
    );

    // A node behind what this device has seen is not trusted with a publish.
    laptop.own_profile_age(next.version + 1, 1_000).unwrap();
    let failure = publish::publish(&laptop, &mut alice.node, &alice.identity, &next)
        .await
        .unwrap_err();
    assert!(matches!(failure, Failure::Node(_)), "{failure}");
    assert_eq!(alice.node.get_profile(&owner).await.unwrap().profile, next);

    // A withdrawal goes past everything, from a draft however old.
    let kept = publish::withdraw(&laptop, &mut alice.node, &alice.identity, &edited)
        .await
        .unwrap();
    assert_eq!(kept.version, next.version + 2);
    let tombstone = alice.node.get_profile(&owner).await.unwrap().profile;
    assert!(tombstone.deleted);
    assert_eq!(tombstone.version, kept.version);
}

#[tokio::test]
async fn a_link_record_crosses_a_node_and_restores_the_owner() {
    const NOW: u64 = 1_000_000;
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let owner = alice.identity.peer_id();
    let mut phone = Store::open(&store_path("restore-phone"), &alice.identity).unwrap();
    phone.set_blocked("bob", true).unwrap();
    phone.set_local_name("carol", "Carrie").unwrap();
    phone.pass("erin", true, 1_000).unwrap();
    // Enough rows for a record over one envelope.
    for peer in 0..4_000 {
        phone
            .set_local_name(&format!("peer-{peer}"), "a contact named here")
            .unwrap();
    }
    phone.set_profile_hidden(true).unwrap();

    let laptop_key = Identity::generate();
    let mut invite = Invite::new(dyapp_client::mailbox(&laptop_key.public_key()), NOW).unwrap();
    let code = Code::parse(&invite.code().text(), NOW).unwrap();
    let mut stranger = Person::device(&address, Identity::generate(), 1).await;
    stranger.node.put(&code.mailbox, vec![7; 64]).await.unwrap();
    let mut inbox = Person::device(&address, Identity::generate(), 1).await.node;
    assert!(invite
        .receive(&mut inbox, &laptop_key, NOW)
        .await
        .unwrap()
        .is_none());

    code.send(
        &mut alice.node,
        &alice.identity,
        &phone.link_state().unwrap(),
        NOW,
    )
    .await
    .unwrap();
    let linked = invite
        .receive(&mut inbox, &laptop_key, NOW)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(linked.identity.peer_id(), owner);
    let (held, more) = inbox.fetch(&laptop_key).await.unwrap();
    assert!(held.is_empty() && !more, "the parts were acked");

    // The fresh store takes the state and the live profile, and the laptop lists itself.
    let mut laptop = Person::device(
        &address,
        Identity::from_secret(&linked.identity.secret()),
        2,
    )
    .await;
    let mut store = Store::open(&store_path("restore-laptop"), &laptop.identity).unwrap();
    let live = alice.node.get_profile(&owner).await.unwrap().profile;
    let mut draft = publish::adopt(
        &mut store,
        &mut laptop.node,
        &alice.identity,
        &linked.state,
        NOW,
    )
    .await
    .unwrap();
    assert!(store.blocked("bob").unwrap());
    assert_eq!(store.local_name("carol").unwrap(), "Carrie");
    assert_eq!(store.passed_peers().unwrap(), ["erin"]);
    assert_eq!(store.own_profile_seen().unwrap(), live.version);

    // The profile stays hidden, and the node serves both devices.
    publish::list_device(&mut draft, laptop.device.certificate().clone());
    let hidden = publish::relist(&store, &mut laptop.node, &alice.identity, &draft)
        .await
        .unwrap();
    assert!(store.profile_hidden().unwrap());
    assert_eq!(hidden.version, live.version + 3);
    let served = alice.node.get_profile(&owner).await.unwrap().profile;
    assert_eq!(
        (served.version, served.deleted, served.devices.len()),
        (live.version + 2, false, 2)
    );
}
