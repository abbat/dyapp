use super::chat::Person;
use super::*;
use dyapp_client::chat::{self, Body, Event};
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_client::{prepare_put, Node};
use std::sync::Arc;

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
    let mut unknown = Person::new(&first_address).await;
    let mut known = Person::new(&first_address).await;
    let mut bob = Person::new(&first_address).await;
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
    let bob_path = protocol_build_path("bob-paged").with_extension("sqlite3");
    let seal_key = [7; 32];
    Store::open(&bob_path, &bob.identity)
        .unwrap()
        .stage_outbound(&bob.device.seal(&seal_key).unwrap(), &[])
        .unwrap();

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

    let protocols = Arc::new(dyapp_p2p_net::Protocols::public());
    let mut second_writer = Node::connect(second_address.clone(), protocols.clone())
        .await
        .unwrap();
    second_writer
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
    second_writer
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
    second_writer.put_prepared(&prepared).await.unwrap();
    let mut second_reader = Node::connect(second_address, protocols).await.unwrap();

    let profiles = [
        known_profile,
        bob.node.get_profile(&bob_owner).await.unwrap(),
    ];
    let mut session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &seal_key).unwrap();
    let (first_events, more) = session
        .receive_page(
            std::slice::from_mut(&mut bob.node),
            &bob.device_key,
            &profiles,
            &seal_key,
        )
        .await
        .unwrap();
    assert!(more);
    assert!(first_events
        .iter()
        .any(|event| matches!(event, Event::UnknownOwner(_))));
    assert!(session
        .store()
        .mailbox_cursor_for(bob.node.peer())
        .unwrap()
        .is_some());
    drop(session);

    let mut session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &seal_key).unwrap();
    assert!(session
        .store()
        .mailbox_cursor_for(bob.node.peer())
        .unwrap()
        .is_some());
    let (joined, more) = session
        .receive_page(
            std::slice::from_mut(&mut second_reader),
            &bob.device_key,
            &profiles,
            &seal_key,
        )
        .await
        .unwrap();
    assert!(!more);
    assert!(joined.iter().any(|event| matches!(event, Event::Joined(_))));
    assert_eq!(
        session.store().mailbox_cursor_for(bob.node.peer()).unwrap(),
        None
    );
    drop(session);

    let mut session =
        Session::open(Store::open(&bob_path, &bob.identity).unwrap(), &seal_key).unwrap();
    let mut text_count = 0;
    let mut reached_end = false;
    for _ in 0..10 {
        let (events, more) = session
            .receive_page(
                std::slice::from_mut(&mut bob.node),
                &bob.device_key,
                &profiles,
                &seal_key,
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
    assert_eq!(remaining[0].ciphertext, unknown_welcome[0].mls);
}
