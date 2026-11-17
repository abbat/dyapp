use super::*;

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
    let (mut alice, mut bob, mut carol, mut profiles) =
        Box::pin(hidden_commit_people(&addresses[0])).await;
    let (mut group, mut bob_groups) = join_hidden_group(&mut alice, &mut bob, &profiles).await;
    let mut second = Box::pin(hide_commit_and_send(
        &mut alice,
        &mut carol,
        &mut group,
        &mut profiles,
        &addresses[1],
    ))
    .await;
    assert_commit_gap(&mut bob, &mut bob_groups, &profiles, &group).await;
    cross_check_hidden_commit(
        &alice,
        &mut bob,
        &mut second,
        &mut bob_groups,
        &profiles,
        &group,
    )
    .await;
}

async fn hidden_commit_people(
    address: &Multiaddr,
) -> (Person, Person, Person, Vec<VerifiedProfile>) {
    let mut alice = Person::new(address).await;
    let mut bob = Person::new(address).await;
    let mut carol = Person::new(address).await;
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
    (alice, bob, carol, profiles)
}

async fn join_hidden_group(
    alice: &mut Person,
    bob: &mut Person,
    profiles: &[VerifiedProfile],
) -> (Group, Vec<Group>) {
    let alice_mailbox = alice.mailbox();
    let (group, welcome) = chat::invite(&mut alice.node, &alice.device, &alice_mailbox, profiles)
        .await
        .unwrap();
    chat::send(&mut alice.node, &welcome).await.unwrap();
    let mut bob_groups = Vec::new();
    let (events, ids) = receive_welcome(bob, &mut bob_groups, profiles).await;
    assert_eq!(events, vec![Event::Joined(group.id().to_vec())]);
    bob.node.ack(&bob.device_key, ids).await.unwrap();
    (group, bob_groups)
}

async fn hide_commit_and_send(
    alice: &mut Person,
    carol: &mut Person,
    group: &mut Group,
    profiles: &mut [VerifiedProfile],
    second_address: &Multiaddr,
) -> dyapp_client::Node {
    // Carol withdraws her device, so Alice drops her; the first node hides the commit, which only
    // the second one holds.
    let withdrawn = Profile {
        version: 2,
        ..Default::default()
    };
    carol
        .node
        .publish_profile(withdrawn.sign(&carol.identity))
        .await
        .unwrap();
    profiles[2] = alice
        .node
        .get_profile(&carol.identity.peer_id())
        .await
        .unwrap();
    let commit = group
        .remove_revoked(&alice.device, profiles)
        .unwrap()
        .unwrap();
    let mut second = Person::device(second_address, Identity::try_generate().unwrap(), 1)
        .await
        .node;
    chat::send(&mut second, &chat::to_members(group, &commit).unwrap())
        .await
        .unwrap();
    let sent = chat::message(
        group,
        &alice.device,
        &Body::text_with_id([1; 16], b"carol is gone".to_vec()),
    )
    .unwrap();
    chat::send(&mut alice.node, &sent).await.unwrap();
    second
}

async fn assert_commit_gap(
    bob: &mut Person,
    bob_groups: &mut Vec<Group>,
    profiles: &[VerifiedProfile],
    group: &Group,
) {
    let (events, ids) = receive_welcome(bob, bob_groups, profiles).await;
    assert_eq!(events, vec![Event::Gap(group.id().to_vec())]);
    assert_eq!(ids.len(), 0);
}

async fn cross_check_hidden_commit(
    alice: &Person,
    bob: &mut Person,
    second: &mut dyapp_client::Node,
    bob_groups: &mut Vec<Group>,
    profiles: &[VerifiedProfile],
    group: &Group,
) {
    let mut scores = dyapp_client::scores::Scores::default();
    let (events, ids) = chat::cross_check(
        &mut scores,
        &mut bob.node,
        second,
        &bob.device_key,
        &bob.device,
        bob_groups,
        profiles,
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
                device: alice.device.device_key().unwrap(),
                body: Body::text_with_id([1; 16], b"carol is gone".to_vec()),
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
    assert_eq!(bob.node.fetch(&bob.device_key).await.unwrap().0.len(), 0);
    assert_eq!(second.fetch(&bob.device_key).await.unwrap().0.len(), 0);
}
