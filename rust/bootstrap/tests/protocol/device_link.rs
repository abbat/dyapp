//! A device linked to an account joins the owner's chats, and a contact holding the older profile
//! catches up instead of falling out of the group.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Body, Event, PeerChange};
use dyapp_client::interactions::Kind;
use dyapp_client::link::{Code, Invite};
use dyapp_client::publish::{self, Failure};
use dyapp_client::session::Session;
use dyapp_client::store::{Conversation, Store};
use dyapp_profile::verify;
use std::slice::from_mut;

const SEAL: [u8; 32] = [6; 32];
/// One device's turn to commit to a group.
const ADD_TURN_SECONDS: u64 = 3600;

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

    // The owner drops the laptop: the member first by device key commits, the other waits its turn.
    let profile = Profile {
        version: 3,
        devices: vec![alice.device.certificate().clone()],
        ..Default::default()
    };
    let dropped = verify(&profile.sign(&alice.identity)).unwrap();
    let dropped = vec![dropped, held[1].clone()];
    let alice_first = alice.device_key.public_key() < bob.device_key.public_key();
    let now = now + ADD_TURN_SECONDS;
    assert_eq!(
        bob_session
            .remove_dropped_devices(from_mut(&mut bob.node), &dropped, now, &SEAL)
            .await
            .unwrap(),
        usize::from(!alice_first)
    );
    assert_eq!(
        alice_session
            .remove_dropped_devices(from_mut(&mut alice.node), &dropped, now, &SEAL)
            .await
            .unwrap(),
        usize::from(alice_first)
    );
    let (second, second_node, key) = match alice_first {
        true => (&mut bob_session, &mut bob.node, &bob.device_key),
        false => (&mut alice_session, &mut alice.node, &alice.device_key),
    };
    let events = second
        .receive(from_mut(second_node), key, &dropped, &SEAL)
        .await
        .unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::Commit(id) if *id == group_id)));
    assert_eq!(
        second
            .remove_dropped_devices(
                from_mut(second_node),
                &dropped,
                now + ADD_TURN_SECONDS,
                &SEAL
            )
            .await
            .unwrap(),
        0
    );
    alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap();
    bob_session
        .receive(from_mut(&mut bob.node), &bob.device_key, &dropped, &SEAL)
        .await
        .unwrap();
    assert!(bob_session.likes(&owner).unwrap().theirs);
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
async fn own_devices_share_a_group_that_is_not_a_chat() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let phone = Person::new(&address).await;
    let owner = phone.identity.peer_id();
    let identity = Identity::from_secret(&phone.identity.secret());
    let laptop = Person::device(&address, identity, 2).await;
    let mut devices = [phone, laptop];
    devices.sort_by_key(Person::mailbox);
    let [mut first, mut second] = devices;
    let profile = Profile {
        version: 2,
        devices: vec![
            first.device.certificate().clone(),
            second.device.certificate().clone(),
        ],
        ..Default::default()
    };
    first
        .node
        .publish_profile(profile.sign(&first.identity))
        .await
        .unwrap();
    for person in [&mut first, &mut second] {
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
    let own = first.node.get_profile(&owner).await.unwrap();
    let held = [own.clone()];
    let now = chrono::Utc::now().timestamp().unsigned_abs();
    let mut first_session = session(&first, "own-first");
    let second_path = store_path("own-second");
    let mut store = Store::open(&second_path, &second.identity).unwrap();
    store
        .stage_outbound(&second.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut second_session = Session::open(store, &SEAL).unwrap();

    // Only the first device by mailbox starts it; the second waits for its turn.
    assert!(!second_session
        .join_own_devices(from_mut(&mut second.node), &own, now, &SEAL)
        .await
        .unwrap());
    assert!(first_session
        .join_own_devices(from_mut(&mut first.node), &own, now, &SEAL)
        .await
        .unwrap());
    let group = first_session.own_group().unwrap();
    let events = second_session
        .receive(from_mut(&mut second.node), &second.device_key, &held, &SEAL)
        .await
        .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Joined(id)] if *id == group),
        "{events:?}"
    );
    assert_eq!(second_session.own_group().as_ref(), Some(&group));
    // A later turn does not start another, and it is no chat on either device.
    assert!(!second_session
        .join_own_devices(from_mut(&mut second.node), &own, now + 86_400, &SEAL)
        .await
        .unwrap());
    first_session
        .receive(from_mut(&mut first.node), &first.device_key, &held, &SEAL)
        .await
        .unwrap();
    for held_here in [&first_session, &second_session] {
        assert!(held_here.store().chat_rows().unwrap().is_empty());
    }

    // A change on one device reaches the other, in both directions.
    let bob = "ab".repeat(32);
    for change in [
        PeerChange::Blocked(true),
        PeerChange::Muted(true),
        PeerChange::Passed(true),
        PeerChange::Name("Bobby".into()),
    ] {
        first_session
            .change_peer(from_mut(&mut first.node), &bob, change, &SEAL)
            .await
            .unwrap();
    }
    second_session
        .receive(from_mut(&mut second.node), &second.device_key, &held, &SEAL)
        .await
        .unwrap();
    let synced = second_session.store();
    assert!(synced.blocked(&bob).unwrap() && synced.muted(&bob).unwrap());
    assert_eq!(synced.passed_peers().unwrap(), vec![bob.clone()]);
    assert_eq!(synced.local_name(&bob).unwrap(), "Bobby");
    second_session
        .change_peer(
            from_mut(&mut second.node),
            &bob,
            PeerChange::Blocked(false),
            &SEAL,
        )
        .await
        .unwrap();
    first_session
        .receive(from_mut(&mut first.node), &first.device_key, &held, &SEAL)
        .await
        .unwrap();
    assert!(!first_session.store().blocked(&bob).unwrap());

    // It survives a restart.
    drop(second_session);
    let store = Store::open(&second_path, &second.identity).unwrap();
    let reopened = Session::open(store, &SEAL).unwrap();
    assert_eq!(reopened.own_group(), Some(group));
}

#[tokio::test]
async fn any_own_device_revokes_another_and_it_stays_out() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let owner = alice.identity.peer_id();
    let alice_session = session(&alice, "revoke-alice");
    let mut laptop =
        Person::device(&address, Identity::from_secret(&alice.identity.secret()), 2).await;
    let mut laptop_session = session(&laptop, "revoke-laptop");
    let laptop_key = laptop.device_key.public_key();

    // Both devices list themselves, with the bindings that close their mailboxes.
    let mut draft = alice.node.get_profile(&owner).await.unwrap().profile;
    draft.device_owners = vec![dyapp_client::owner_binding(
        &alice.device_key,
        &alice.identity.public_key(),
    )];
    let before_laptop = draft.clone();
    let binding = dyapp_client::owner_binding(&laptop.device_key, &alice.identity.public_key());
    laptop_session.list_self(&mut draft, binding).unwrap();
    let store = alice_session.store();
    let listed = publish::relist(store, &mut alice.node, &alice.identity, &draft)
        .await
        .unwrap();
    assert!(!laptop_session.own_device_revoked(&listed).unwrap());
    assert!(alice.node.fetch(&laptop.device_key).await.is_ok());

    // A refusal alone wipes nothing while the newest profile lists the device.
    let refused =
        || dyapp_client::session::Error::Client(dyapp_client::Error::Status(Status::Refused));
    let error = laptop_session.revoked_or(&mut laptop.node, refused()).await;
    assert!(
        matches!(error, dyapp_client::session::Error::Client(_)),
        "{error}"
    );

    // A profile without a binding for the device closes nothing and changes nothing.
    let failure =
        publish::revoke_device(store, &mut alice.node, &alice.identity, &listed, &[9; 32])
            .await
            .unwrap_err();
    assert!(matches!(failure, Failure::Node(_)), "{failure}");
    assert_eq!(
        alice.node.get_profile(&owner).await.unwrap().profile,
        listed
    );

    // The first device drops the laptop: the node refuses its mailbox and lists only the first. Its
    // draft predates the laptop, and the newest profile at the node takes its place.
    let dropped = publish::revoke_device(
        store,
        &mut alice.node,
        &alice.identity,
        &before_laptop,
        &laptop_key,
    )
    .await
    .unwrap();
    assert_eq!(dropped.version, listed.version + 1);
    assert_eq!(dropped.devices, [alice.device.certificate().clone()]);
    assert_eq!(
        alice.node.get_profile(&owner).await.unwrap().profile,
        dropped
    );
    assert!(alice.node.fetch(&laptop.device_key).await.is_err());
    assert!(alice.node.fetch(&alice.device_key).await.is_ok());

    // The laptop sees it, ignores the older copy, and cannot list itself again.
    assert!(laptop_session.own_device_revoked(&dropped).unwrap());
    assert!(!laptop_session.own_device_revoked(&listed).unwrap());
    let mut draft = dropped.clone();
    let binding = dyapp_client::owner_binding(&laptop.device_key, &alice.identity.public_key());
    assert!(laptop_session.list_self(&mut draft, binding).is_err());
    assert_eq!(draft, dropped);

    // The closed mailbox and the signed profile together tell the laptop to wipe itself.
    let error = laptop_session
        .receive_page(from_mut(&mut laptop.node), &laptop.device_key, &[], &SEAL)
        .await
        .err()
        .unwrap();
    let error = laptop_session.revoked_or(&mut laptop.node, error).await;
    assert!(
        matches!(error, dyapp_client::session::Error::Revoked),
        "{error}"
    );
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
    let binding = dyapp_client::owner_binding(&laptop.device_key, &alice.identity.public_key());
    publish::list_device(
        &mut draft,
        laptop.device.certificate().clone(),
        binding.clone(),
    );
    let hidden = publish::relist(&store, &mut laptop.node, &alice.identity, &draft)
        .await
        .unwrap();
    assert!(store.profile_hidden().unwrap());
    assert_eq!(hidden.version, live.version + 3);
    assert!(hidden.device_owners.contains(&binding));
    let served = alice.node.get_profile(&owner).await.unwrap().profile;
    assert_eq!(
        (served.version, served.deleted, served.devices.len()),
        (live.version + 2, false, 2)
    );
}

#[tokio::test]
async fn a_linked_device_finishes_after_a_failed_step_without_spending_a_version() {
    const NOW: u64 = 1_000_000;
    const PACKAGES: usize = 2;
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let owner = alice.identity.peer_id();
    let device_key = Identity::generate();
    let copy = Identity::from_secret(&alice.identity.secret());
    let path = store_path("finish-laptop");
    let mut laptop = Person::device(&address, copy, 2).await;
    let identity = Identity::from_secret(&alice.identity.secret());
    let store = Store::open(&path, &identity).unwrap();
    assert!(!Session::finish_link(
        store,
        &mut laptop.node,
        &identity,
        &device_key,
        PACKAGES,
        &SEAL,
        NOW
    )
    .await
    .unwrap());

    // A step fails: no profile answers, and the state stays for the next pass.
    let store = Store::open(&path, &identity).unwrap();
    let phone = Store::open(&store_path("finish-phone"), &identity).unwrap();
    phone.set_blocked("bob", true).unwrap();
    store
        .set_link_pending(&phone.link_state().unwrap())
        .unwrap();
    let stranger = Identity::generate();
    assert!(Session::finish_link(
        store,
        &mut laptop.node,
        &stranger,
        &device_key,
        PACKAGES,
        &SEAL,
        NOW
    )
    .await
    .is_err());

    let store = Store::open(&path, &identity).unwrap();
    assert!(store.link_pending().unwrap().is_some());
    let live = alice.node.get_profile(&owner).await.unwrap().profile;
    assert!(Session::finish_link(
        store,
        &mut laptop.node,
        &identity,
        &device_key,
        PACKAGES,
        &SEAL,
        NOW
    )
    .await
    .unwrap());
    let listed = alice.node.get_profile(&owner).await.unwrap().profile;
    assert_eq!(listed.version, live.version + 1);
    assert_eq!(listed.devices.len(), live.devices.len() + 1);

    // Everything is done but the row: a repeat ends it and leaves the profile alone.
    let store = Store::open(&path, &identity).unwrap();
    assert!(store.link_pending().unwrap().is_none());
    assert!(store.blocked("bob").unwrap());
    assert!(store.device_listed().unwrap() >= listed.version);
    store
        .set_link_pending(&phone.link_state().unwrap())
        .unwrap();
    assert!(Session::finish_link(
        store,
        &mut laptop.node,
        &identity,
        &device_key,
        PACKAGES,
        &SEAL,
        NOW
    )
    .await
    .unwrap());
    let served = alice.node.get_profile(&owner).await.unwrap().profile;
    assert_eq!(served.version, listed.version);
}
