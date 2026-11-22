//! Likes travel as opaque signal envelopes and match once both owners like each other.
use super::chat::Person;
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
fn open(person: &Person, name: &str) -> (dyapp_test_support::TestDir, Session) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let mut store = Store::open(
        &test_dir.path().join(format!("{name}.db")),
        &person.identity,
    )
    .unwrap();
    store
        .stage_outbound(&person.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    (test_dir, Session::open(store, &SEAL).unwrap())
}

/// The ID of the group `events` joined.
fn joined(events: &[Event]) -> Vec<u8> {
    let id = events.iter().find_map(|event| match event {
        Event::Joined(id) => Some(id.clone()),
        _ => None,
    });
    id.unwrap()
}

async fn publish_packages(person: &mut Person, session: &mut Session) {
    session
        .publish_key_packages(&mut person.node, &person.device_key, 1, &SEAL)
        .await
        .unwrap();
}

async fn send_text(
    session: &mut Session,
    person: &mut Person,
    profiles: &[dyapp_profile::VerifiedProfile],
    text: &[u8],
) -> [u8; 16] {
    session
        .send_text_to(from_mut(&mut person.node), profiles, text, &SEAL)
        .await
        .unwrap()
}

async fn interaction_profiles(
    alice: &mut Person,
    alice_owner: &str,
    bob_owner: &str,
    carol_owner: &str,
) -> (
    Vec<dyapp_profile::VerifiedProfile>,
    [dyapp_profile::VerifiedProfile; 2],
) {
    let pair = vec![
        alice.node.get_profile(alice_owner).await.unwrap(),
        alice.node.get_profile(bob_owner).await.unwrap(),
    ];
    let others = [
        pair[0].clone(),
        alice.node.get_profile(carol_owner).await.unwrap(),
    ];
    (pair, others)
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
    let (pair, others) =
        interaction_profiles(&mut alice, &alice_owner, &bob_owner, &carol_owner).await;
    let (_test_dir_107, mut alice_session) = open(&alice, "first-alice");
    let (_test_dir_108, mut bob_session) = open(&bob, "first-bob");
    let (_test_dir_109, mut carol_session) = open(&carol, "first-carol");
    publish_packages(&mut bob, &mut bob_session).await;
    publish_packages(&mut carol, &mut carol_session).await;

    // Only a like invites: a view with no group sends nothing.
    assert!(alice_session
        .interact_with(from_mut(&mut alice.node), &pair, Kind::View, &SEAL)
        .await
        .unwrap()
        .is_none());
    assert_eq!(receive(&mut bob_session, &mut bob, &pair).await.len(), 0);

    // The like carries its Welcome, and Bob's reply goes to the group it made.
    alice_session
        .interact_with(from_mut(&mut alice.node), &pair, Kind::Like, &SEAL)
        .await
        .unwrap()
        .unwrap();
    let group = joined(&receive(&mut bob_session, &mut bob, &pair).await);
    assert!(bob_session.likes(&alice_owner).unwrap().theirs);
    let id = send_text(&mut bob_session, &mut bob, &pair, b"hi").await;
    receive(&mut alice_session, &mut alice, &pair).await;
    let reply = MessageKey {
        group,
        author: bob_owner,
        id,
    };
    assert!(alice_session.chat_message(&reply).unwrap().is_some());

    // A first text fetches the KeyPackage and carries its Welcome the same way.
    let id = send_text(&mut alice_session, &mut alice, &others, b"hello").await;
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
    let (_test_dir, mut alice_session, mut bob_session, profiles, group_id, bob_path) =
        open_like_sessions(&mut alice, &mut bob, &bob_owner).await;
    assert_opaque_signal(
        &mut alice,
        &mut bob,
        &mut alice_session,
        &mut bob_session,
        &profiles,
        &group_id,
        &alice_owner,
    )
    .await;

    // Bob likes back: both see a match.
    send_interaction(&mut bob_session, &mut bob, &group_id, Kind::Like)
        .await
        .unwrap();
    receive(&mut alice_session, &mut alice, &profiles).await;
    assert!(alice_session.likes(&bob_owner).unwrap().matched());
    assert!(bob_session.likes(&alice_owner).unwrap().matched());

    bob_session = retry_unlike(&mut bob, bob_session, &bob_path, &group_id, &alice_owner).await;
    assert_unlike_received(
        &mut alice,
        &mut alice_session,
        &bob_session,
        &profiles,
        &alice_owner,
        &bob_owner,
    )
    .await;
    assert_daily_view(
        &mut alice,
        &mut bob,
        &mut alice_session,
        &mut bob_session,
        &profiles,
        &group_id,
        &alice_owner,
    )
    .await;
}

async fn like_profiles(
    alice: &mut Person,
    bob: &mut Person,
    bob_owner: &str,
) -> Vec<dyapp_profile::VerifiedProfile> {
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, bob_owner, 1)
        .await
        .unwrap();
    let mut profiles = Vec::new();
    for owner in [alice.identity.peer_id(), bob_owner.to_owned()] {
        profiles.push(alice.node.get_profile(&owner).await.unwrap());
    }
    profiles
}

async fn open_like_sessions(
    alice: &mut Person,
    bob: &mut Person,
    bob_owner: &str,
) -> (
    dyapp_test_support::TestDir,
    Session,
    Session,
    Vec<dyapp_profile::VerifiedProfile>,
    Vec<u8>,
    std::path::PathBuf,
) {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let profiles = like_profiles(alice, bob, bob_owner).await;
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, &profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let group_id = group.id().to_vec();
    let mut store = Store::open(&test_dir.path().join("likes-alice.db"), &alice.identity).unwrap();
    store.set_send_profile_views(true).unwrap();
    let sealed = alice.device.seal(&SEAL).unwrap();
    store
        .stage_group(&group, &profiles[1], &sealed, &[])
        .unwrap();
    let mut alice_session = Session::open(store, &SEAL).unwrap();
    let bob_path = test_dir.path().join("likes-bob.db");
    let mut store = Store::open(&bob_path, &bob.identity).unwrap();
    store
        .stage_outbound(&bob.device.seal(&SEAL).unwrap(), &[])
        .unwrap();
    let mut bob_session = Session::open(store, &SEAL).unwrap();
    for _ in 0..2 {
        receive(&mut bob_session, bob, &profiles).await;
        receive(&mut alice_session, alice, &profiles).await;
    }
    (
        test_dir,
        alice_session,
        bob_session,
        profiles,
        group_id,
        bob_path,
    )
}

async fn send_interaction(
    session: &mut Session,
    person: &mut Person,
    group_id: &[u8],
    kind: Kind,
) -> Option<dyapp_client::interactions::Interaction> {
    session
        .interact(from_mut(&mut person.node), group_id, kind, &SEAL)
        .await
        .unwrap()
}

fn interaction_event(
    group_id: &[u8],
    sender: &str,
    device: Vec<u8>,
    event: dyapp_client::interactions::Interaction,
) -> Event {
    Event::Interaction {
        group: group_id.to_vec(),
        sender: sender.to_owned(),
        device,
        event,
    }
}

async fn assert_opaque_signal(
    alice: &mut Person,
    bob: &mut Person,
    alice_session: &mut Session,
    bob_session: &mut Session,
    profiles: &[dyapp_profile::VerifiedProfile],
    group_id: &[u8],
    alice_owner: &str,
) {
    // Alice's like reaches the node as a signal whose ciphertext hides the event.
    let like = send_interaction(alice_session, alice, group_id, Kind::Like)
        .await
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
    assert_ne!(envelopes.len(), 0);
    assert!(envelopes.iter().all(|envelope| !envelope
        .ciphertext
        .windows(16)
        .any(|window| window == like.event_id)));
    assert_eq!(
        receive(bob_session, bob, profiles).await,
        [interaction_event(
            group_id,
            alice_owner,
            alice.device.device_key().unwrap(),
            like,
        )]
    );
    assert!(!bob_session.likes(alice_owner).unwrap().matched());
    // A repeated put of the same signal changes nothing.
    for write in writes {
        alice.node.put_prepared(&write).await.unwrap();
    }
    assert_eq!(receive(bob_session, bob, profiles).await.len(), 0);
}

async fn retry_unlike(
    bob: &mut Person,
    mut session: Session,
    path: &std::path::Path,
    group_id: &[u8],
    alice_owner: &str,
) -> Session {
    // Bob's unlike, sent offline, is retried later and ends the match for Alice.
    assert!(matches!(
        session
            .interact(&mut [], group_id, Kind::Unlike, &SEAL)
            .await,
        Err(Error::Client(_))
    ));
    assert!(!session.likes(alice_owner).unwrap().matched());
    drop(session);
    let store = Store::open(path, &bob.identity).unwrap();
    let mut session = Session::open(store, &SEAL).unwrap();
    assert!(!session.likes(alice_owner).unwrap().matched());
    let now_ms = super::now_ms();
    session
        .retry_due(from_mut(&mut bob.node), now_ms + 120_000, &SEAL)
        .await
        .unwrap();
    session
}

async fn assert_unlike_received(
    alice: &mut Person,
    alice_session: &mut Session,
    bob_session: &Session,
    profiles: &[dyapp_profile::VerifiedProfile],
    alice_owner: &str,
    bob_owner: &str,
) {
    let events = receive(alice_session, alice, profiles).await;
    assert!(matches!(
        events.as_slice(),
        [Event::Interaction { event, .. }] if event.kind == Kind::Unlike
    ));
    assert!(!alice_session.likes(bob_owner).unwrap().matched());
    for (session, peer) in [(&*alice_session, bob_owner), (bob_session, alice_owner)] {
        let state = session.conversation(peer).unwrap();
        assert_eq!(state, dyapp_client::store::Conversation::Closed);
    }
}

async fn assert_daily_view(
    alice: &mut Person,
    bob: &mut Person,
    alice_session: &mut Session,
    bob_session: &mut Session,
    profiles: &[dyapp_profile::VerifiedProfile],
    group_id: &[u8],
    alice_owner: &str,
) {
    // Alice sends one view a day; Bob receives it without sending views himself.
    let view = send_interaction(alice_session, alice, group_id, Kind::View)
        .await
        .unwrap();
    assert!(send_interaction(alice_session, alice, group_id, Kind::View)
        .await
        .is_none());
    assert_eq!(
        receive(bob_session, bob, profiles).await,
        [interaction_event(
            group_id,
            alice_owner,
            alice.device.device_key().unwrap(),
            view,
        )]
    );
    assert!(bob_session
        .interact(&mut [], group_id, Kind::View, &SEAL)
        .await
        .unwrap()
        .is_none());
}
