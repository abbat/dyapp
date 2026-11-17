use super::chat::Person;
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_client::{prepare_put, Node};
use std::sync::Arc;

/// A restart empties the profile cache, so a backlog's first page waits for its sender's profile.
/// Reading on before going back would open later texts first and lose that page for good.
#[tokio::test]
async fn a_backlog_longer_than_a_page_survives_a_cold_profile_cache() {
    let address = start_config(|config| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.require_write_tokens = false;
        config.limits.min_free_mb = 0;
    })
    .await;
    let mut alice = Person::new(&address).await;
    let mut bob = Person::new(&address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 2)
        .await
        .unwrap();
    let bob_profile = bob.node.get_profile(&bob_owner).await.unwrap();
    let alice_profile = alice
        .node
        .get_profile(&alice.identity.peer_id())
        .await
        .unwrap();
    let bob_path = protocol_build_path("bob-backlog").with_extension("sqlite3");
    let seal_key = [7; 32];
    Store::open(&bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&seal_key).unwrap(), &[])
        .unwrap();
    let mut session = establish_backlog(
        &mut alice,
        &mut bob,
        &alice_profile,
        &bob_profile,
        &bob_path,
        &seal_key,
    )
    .await;
    drain_backlog(
        &mut bob,
        &mut session,
        alice_profile,
        bob_profile,
        &seal_key,
    )
    .await;
    let (remaining, more) = bob.node.fetch(&bob.device_key).await.unwrap();
    assert!(remaining.is_empty() && !more);
}

async fn establish_backlog(
    alice: &mut Person,
    bob: &mut Person,
    alice_profile: &dyapp_profile::VerifiedProfile,
    bob_profile: &dyapp_profile::VerifiedProfile,
    bob_path: &std::path::Path,
    seal_key: &[u8; 32],
) -> Session {
    let mailbox = alice.mailbox();
    let (mut group, welcome) = chat::invite(
        &mut alice.node,
        &alice.device,
        &mailbox,
        &[alice_profile.clone(), bob_profile.clone()],
    )
    .await
    .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let mut session =
        Session::open(Store::open(bob_path, &bob.identity).unwrap(), seal_key).unwrap();
    let (joined, _) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &[alice_profile.clone(), bob_profile.clone()],
            seal_key,
        )
        .await
        .unwrap();
    assert!(joined.iter().any(|event| matches!(event, Event::Joined(_))));
    for i in 0..45u8 {
        let body = Body::text_with_id([i + 1; 16], vec![i]);
        let text = chat::message(&mut group, &alice.device, &body).unwrap();
        chat::send(&mut alice.node, &text).await.unwrap();
    }
    session
}

async fn drain_backlog(
    bob: &mut Person,
    session: &mut Session,
    alice_profile: dyapp_profile::VerifiedProfile,
    bob_profile: dyapp_profile::VerifiedProfile,
    seal_key: &[u8; 32],
) {
    let mut profiles = vec![bob_profile];
    for _ in 0..8 {
        let (events, more) = session
            .receive_page(
                std::slice::from_mut(&mut bob.node),
                &bob.device_key,
                &profiles,
                seal_key,
            )
            .await
            .unwrap();
        let mut resolved = false;
        for event in events {
            match event {
                Event::Refused(why) => panic!("refused: {why}"),
                Event::UnknownOwner(_) if !resolved => {
                    profiles.push(alice_profile.clone());
                    resolved = true;
                }
                _ => {}
            }
        }
        if !session.next_page(more, resolved).unwrap() {
            break;
        }
    }
}

#[tokio::test]
async fn unresolved_head_does_not_hide_later_mail_after_restart_and_holder_switch() {
    let configure = |config: &mut NodeConfig| {
        config.roles = vec![Role::Store];
        config.limits.requests_per_second = 1000;
        config.limits.require_write_tokens = false;
        config.limits.min_free_mb = 0;
    };
    let first_address = start_config(configure).await;
    let second_address = start_config(configure).await;
    Box::pin(run_paged_mail_scenario(first_address, second_address)).await;
}

async fn run_paged_mail_scenario(first_address: Multiaddr, second_address: Multiaddr) {
    let (mut unknown, mut known, mut bob, unknown_profile, known_profile, bob_profile) =
        Box::pin(paged_people(&first_address)).await;
    let bob_owner = bob.identity.peer_id();
    let bob_path = protocol_build_path("bob-paged").with_extension("sqlite3");
    let seal_key = [7; 32];
    Store::open(&bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&seal_key).unwrap(), &[])
        .unwrap();
    let (prepared, unknown_ciphertext) = send_paged_mail(
        &mut unknown,
        &mut known,
        &mut bob,
        unknown_profile,
        &known_profile,
        bob_profile,
    )
    .await;
    let mut second_reader = Box::pin(copy_to_second_holder(
        second_address,
        &bob,
        &known,
        &prepared,
    ))
    .await;
    let profiles = [
        known_profile,
        bob.node.get_profile(&bob_owner).await.unwrap(),
    ];
    assert_unresolved_head(&mut bob, &bob_path, &seal_key, &profiles).await;
    assert_second_holder(
        &mut bob,
        &mut second_reader,
        &bob_path,
        &seal_key,
        &profiles,
    )
    .await;
    assert_late_text_after_restart(
        &mut bob,
        &bob_path,
        &seal_key,
        &profiles,
        unknown_ciphertext,
    )
    .await;
}

async fn paged_people(
    address: &Multiaddr,
) -> (
    Person,
    Person,
    Person,
    dyapp_profile::VerifiedProfile,
    dyapp_profile::VerifiedProfile,
    dyapp_profile::VerifiedProfile,
) {
    let mut unknown = Person::new(address).await;
    let mut known = Person::new(address).await;
    let mut bob = Person::new(address).await;
    let bob_owner = bob.identity.peer_id();
    chat::publish_key_packages(&mut bob.node, &bob.device_key, &bob.device, &bob_owner, 2)
        .await
        .unwrap();
    let bob_profile = bob.node.get_profile(&bob_owner).await.unwrap();
    let unknown_profile = unknown
        .node
        .get_profile(&unknown.identity.peer_id())
        .await
        .unwrap();
    let known_profile = known
        .node
        .get_profile(&known.identity.peer_id())
        .await
        .unwrap();
    (
        unknown,
        known,
        bob,
        unknown_profile,
        known_profile,
        bob_profile,
    )
}

async fn send_paged_mail(
    unknown: &mut Person,
    known: &mut Person,
    bob: &mut Person,
    unknown_profile: dyapp_profile::VerifiedProfile,
    known_profile: &dyapp_profile::VerifiedProfile,
    bob_profile: dyapp_profile::VerifiedProfile,
) -> (dyapp_client::PreparedEnvelope, Vec<u8>) {
    let unknown_mailbox = unknown.mailbox();
    let (_, unknown_welcome) = chat::invite(
        &mut unknown.node,
        &unknown.device,
        &unknown_mailbox,
        &[unknown_profile, bob_profile.clone()],
    )
    .await
    .unwrap();
    chat::send(&mut unknown.node, &unknown_welcome)
        .await
        .unwrap();
    for _ in 0..101 {
        unknown.node.put(&bob.mailbox(), vec![0xff]).await.unwrap();
    }
    let known_mailbox = known.mailbox();
    let (mut known_group, known_welcome) = chat::invite(
        &mut known.node,
        &known.device,
        &known_mailbox,
        &[known_profile.clone(), bob_profile],
    )
    .await
    .unwrap();
    assert_eq!(known_welcome.len(), 1);
    let prepared = prepare_put(&known_welcome[0].mailbox, known_welcome[0].mls.clone()).unwrap();
    known.node.put_prepared(&prepared).await.unwrap();
    let late = chat::message(
        &mut known_group,
        &known.device,
        &Body::text_with_id([9; 16], b"late valid text".to_vec()),
    )
    .unwrap();
    chat::send(&mut known.node, &late).await.unwrap();
    (prepared, unknown_welcome[0].mls.clone())
}

async fn copy_to_second_holder(
    address: Multiaddr,
    bob: &Person,
    known: &Person,
    prepared: &dyapp_client::PreparedEnvelope,
) -> Node {
    let protocols = Arc::new(dyapp_p2p_net::Protocols::public());
    let mut writer = Node::connect(address.clone(), protocols.clone())
        .await
        .unwrap();
    writer
        .publish_profile(
            Profile {
                version: 1,
                devices: vec![bob.device.certificate().clone()],
                ..Default::default()
            }
            .sign(&bob.identity),
        )
        .await
        .unwrap();
    writer
        .publish_profile(
            Profile {
                version: 1,
                devices: vec![known.device.certificate().clone()],
                ..Default::default()
            }
            .sign(&known.identity),
        )
        .await
        .unwrap();
    writer.put_prepared(prepared).await.unwrap();
    Node::connect(address, protocols).await.unwrap()
}

async fn assert_unresolved_head(
    bob: &mut Person,
    path: &std::path::Path,
    seal_key: &[u8; 32],
    profiles: &[dyapp_profile::VerifiedProfile],
) {
    let mut session = Session::open(Store::open(path, &bob.identity).unwrap(), seal_key).unwrap();
    let (events, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            profiles,
            seal_key,
        )
        .await
        .unwrap();
    assert!(more);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::UnknownOwner(_))));
    assert!(session
        .store()
        .mailbox_cursor_for(bob.node.peer())
        .unwrap()
        .is_some());
}

async fn assert_second_holder(
    bob: &mut Person,
    reader: &mut Node,
    path: &std::path::Path,
    seal_key: &[u8; 32],
    profiles: &[dyapp_profile::VerifiedProfile],
) {
    let mut session = Session::open(Store::open(path, &bob.identity).unwrap(), seal_key).unwrap();
    assert!(session
        .store()
        .mailbox_cursor_for(bob.node.peer())
        .unwrap()
        .is_some());
    let (joined, more) = session
        .receive_page(
            std::slice::from_mut(reader),
            &bob.device_key,
            profiles,
            seal_key,
        )
        .await
        .unwrap();
    assert!(!more);
    assert!(joined.iter().any(|event| matches!(event, Event::Joined(_))));
    assert_eq!(
        session.store().mailbox_cursor_for(bob.node.peer()).unwrap(),
        None
    );
}

async fn assert_late_text_after_restart(
    bob: &mut Person,
    path: &std::path::Path,
    seal_key: &[u8; 32],
    profiles: &[dyapp_profile::VerifiedProfile],
    unknown_ciphertext: Vec<u8>,
) {
    let mut session = Session::open(Store::open(path, &bob.identity).unwrap(), seal_key).unwrap();
    let mut text_count = 0;
    let mut reached_end = false;
    for _ in 0..10 {
        let (events, more) = session
            .receive_page(
                std::slice::from_mut(&mut bob.node),
                &bob.device_key,
                profiles,
                seal_key,
            )
            .await
            .unwrap();
        text_count += events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Message {
                        body: Body::Text { text, .. },
                        ..
                    } if text == b"late valid text"
                )
            })
            .count();
        if !more {
            reached_end = true;
            break;
        }
    }
    assert!(reached_end);
    assert_eq!(text_count, 1);
    assert_eq!(
        session.store().mailbox_cursor_for(bob.node.peer()).unwrap(),
        None
    );
    let (remaining, _) = bob.node.fetch(&bob.device_key).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].ciphertext, unknown_ciphertext);
}
