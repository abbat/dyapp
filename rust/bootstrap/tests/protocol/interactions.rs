//! Likes travel as opaque signal envelopes and match once both owners like each other.
use super::chat::{store_path, Person};
use super::*;
use dyapp_client::chat::{self, Event};
use dyapp_client::interactions::Kind;
use dyapp_client::session::{Error, Session};
use dyapp_client::store::messages::MessageKey;
use dyapp_client::store::Store;
use dyapp_p2p_net::proto::EnvelopeClass;
use std::slice::from_mut;

const SEAL: [u8; 32] = [5; 32];

async fn receive(
    session: &mut Session,
    person: &mut Person,
    profiles: &[dyapp_profile::VerifiedProfile],
) -> Vec<Event> {
    session
        .receive(
            from_mut(&mut person.node),
            &person.device_key,
            profiles,
            &SEAL,
        )
        .await
        .unwrap()
}

/// `person`'s session over a fresh store holding their device.
fn open(person: &Person, name: &str) -> Session {
    let mut store = Store::open(&store_path(name), &person.identity).unwrap();
    store
        .stage_outbound(&person.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    Session::open(store, &SEAL).unwrap()
}

/// The ID of the group `events` joined.
fn joined(events: &[Event]) -> Vec<u8> {
    let id = events.iter().find_map(|event| match event {
        Event::Joined(id) => Some(id.clone()),
        _ => None,
    });
    id.unwrap()
}

#[tokio::test]
async fn a_first_like_and_a_first_text_invite_a_found_peer() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let mut carol = Person::new(&address).await;
    let (alice_owner, bob_owner) = (alice.identity.peer_id(), bob.identity.peer_id());
    let carol_owner = carol.identity.peer_id();
    let mut pair = Vec::new();
    for owner in [&alice_owner, &bob_owner] {
        pair.push(alice.node.get_profile(owner).await.unwrap());
    }
    let others = [
        pair[0].clone(),
        alice.node.get_profile(&carol_owner).await.unwrap(),
    ];
    let mut alice_session = open(&alice, "first-alice");
    let mut bob_session = open(&bob, "first-bob");
    let mut carol_session = open(&carol, "first-carol");
    bob_session
        .publish_key_packages(&mut bob.node, &bob.device_key, 1, &SEAL)
        .await
        .unwrap();
    carol_session
        .publish_key_packages(&mut carol.node, &carol.device_key, 1, &SEAL)
        .await
        .unwrap();

    // Only a like invites: a view with no group sends nothing.
    assert!(alice_session
        .interact_with(from_mut(&mut alice.node), &pair, Kind::View, &SEAL)
        .await
        .unwrap()
        .is_none());
    assert!(receive(&mut bob_session, &mut bob, &pair).await.is_empty());

    // The like carries its Welcome, and Bob's reply goes to the group it made.
    alice_session
        .interact_with(from_mut(&mut alice.node), &pair, Kind::Like, &SEAL)
        .await
        .unwrap()
        .unwrap();
    let group = joined(&receive(&mut bob_session, &mut bob, &pair).await);
    assert!(bob_session.likes(&alice_owner).unwrap().theirs);
    let id = bob_session
        .send_text_to(from_mut(&mut bob.node), &pair, b"hi", &SEAL)
        .await
        .unwrap();
    receive(&mut alice_session, &mut alice, &pair).await;
    let reply = MessageKey {
        group,
        author: bob_owner,
        id,
    };
    assert!(alice_session.chat_message(&reply).unwrap().is_some());

    // A first text fetches the KeyPackage and carries its Welcome the same way.
    let id = alice_session
        .send_text_to(from_mut(&mut alice.node), &others, b"hello", &SEAL)
        .await
        .unwrap();
    let group = joined(&receive(&mut carol_session, &mut carol, &others).await);
    let text = MessageKey {
        group,
        author: alice_owner,
        id,
    };
    assert!(carol_session.chat_message(&text).unwrap().is_some());
}

#[tokio::test]
async fn likes_match_unlike_and_retry_as_opaque_signals() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.min_free_mb = 0;
        config.limits.require_write_tokens = false;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let (alice_owner, bob_owner) = (alice.identity.peer_id(), bob.identity.peer_id());
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice_owner.clone(), bob_owner.clone()] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let group_id = group.id().to_vec();
    let mut store = Store::open(&store_path("likes-alice"), &alice.identity).unwrap();
    store.set_send_profile_views(true).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let mut alice_session = Session::open(store, &SEAL).unwrap();
    let bob_path = store_path("likes-bob");
    let mut store = Store::open(&bob_path, &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();
    for _ in 0..2 {
        receive(&mut bob_session, &mut bob, &profiles).await;
        receive(&mut alice_session, &mut alice, &profiles).await;
    }

    // Alice's like reaches the node as a signal whose ciphertext hides the event.
    let like = alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap()
        .unwrap();
    let writes = alice_session.due_outbound(i64::MAX).unwrap();
    let classes = writes.iter().map(|write| {
        let record = SignedRecord::decode(write.as_bytes()).unwrap();
        proto::Envelope::decode(record.payload.as_slice())
            .unwrap()
            .class()
    });
    assert!(classes.collect::<Vec<_>>().contains(&EnvelopeClass::Signal));
    let (envelopes, _) = bob.node.fetch(&bob.device_key).await.unwrap();
    assert!(!envelopes.is_empty());
    assert!(envelopes.iter().all(|envelope| !envelope
        .ciphertext
        .windows(16)
        .any(|window| window == like.event_id)));
    let liked = |sender: &str, device: Vec<u8>, event| Event::Interaction {
        group: group_id.clone(),
        sender: sender.to_owned(),
        device,
        event,
    };
    assert_eq!(
        receive(&mut bob_session, &mut bob, &profiles).await,
        [liked(
            &alice_owner,
            alice.device.device_key().unwrap(),
            like
        )]
    );
    assert!(!bob_session.likes(&alice_owner).unwrap().matched());

    // A repeated put of the same signal changes nothing.
    for write in writes {
        alice.node.put_prepared(&write).await.unwrap();
    }
    assert!(receive(&mut bob_session, &mut bob, &profiles)
        .await
        .is_empty());

    // Bob likes back: both see a match.
    bob_session
        .interact(from_mut(&mut bob.node), &group_id, Kind::Like, &SEAL)
        .await
        .unwrap()
        .unwrap();
    receive(&mut alice_session, &mut alice, &profiles).await;
    assert!(alice_session.likes(&bob_owner).unwrap().matched());
    assert!(bob_session.likes(&alice_owner).unwrap().matched());

    // Bob's unlike, sent offline, is retried later and ends the match for Alice.
    assert!(matches!(
        bob_session
            .interact(&mut [], &group_id, Kind::Unlike, &SEAL)
            .await,
        Err(Error::Client(_))
    ));
    assert!(!bob_session.likes(&alice_owner).unwrap().matched());
    drop(bob_session);
    let store = Store::open(&bob_path, &bob.identity).unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();
    assert!(!bob_session.likes(&alice_owner).unwrap().matched());
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    bob_session
        .retry_due(from_mut(&mut bob.node), now_ms + 120_000, &SEAL)
        .await
        .unwrap();
    let events = receive(&mut alice_session, &mut alice, &profiles).await;
    assert!(matches!(
        events.as_slice(),
        [Event::Interaction { event, .. }] if event.kind == Kind::Unlike
    ));
    assert!(!alice_session.likes(&bob_owner).unwrap().matched());

    // Alice sends one view a day; Bob receives it without sending views himself.
    let view = alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::View, &SEAL)
        .await
        .unwrap()
        .unwrap();
    assert!(alice_session
        .interact(from_mut(&mut alice.node), &group_id, Kind::View, &SEAL)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        receive(&mut bob_session, &mut bob, &profiles).await,
        [liked(
            &alice_owner,
            alice.device.device_key().unwrap(),
            view
        )]
    );
    assert!(bob_session
        .interact(&mut [], &group_id, Kind::View, &SEAL)
        .await
        .unwrap()
        .is_none());
}
