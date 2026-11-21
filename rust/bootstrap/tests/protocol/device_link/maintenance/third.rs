//! A later own device must reuse the private group before publishing private changes.
use super::*;

async fn publish_third(
    first: &mut Person,
    second: &Person,
    address: &Multiaddr,
) -> (Person, dyapp_profile::VerifiedProfile) {
    let identity = Identity::from_secret(&first.identity.secret());
    let mut third = Person::device(address, identity, 3).await;
    let mut devices = [&*first, second, &third];
    devices.sort_by_key(|person| person.mailbox());
    let profile = Profile {
        version: 3,
        devices: devices
            .iter()
            .map(|person| person.device.certificate().clone())
            .collect(),
        ..Default::default()
    };
    first
        .node
        .publish_profile(profile.sign(&first.identity))
        .await
        .unwrap();
    chat::publish_key_packages(
        &mut third.node,
        &third.device_key,
        &third.device,
        &first.identity.peer_id(),
        1,
    )
    .await
    .unwrap();
    let own = first
        .node
        .get_profile(&first.identity.peer_id())
        .await
        .unwrap();
    (third, own)
}

async fn maintain_existing(
    participants: [DeviceSession<'_>; 2],
    own: &dyapp_profile::VerifiedProfile,
) {
    let held = [own.clone()];
    // Both current members observe the same transition clock. Raw device-key order, not
    // mailbox order, determines the one member allowed to commit the addition immediately.
    for (session, person) in participants {
        session
            .maintain_own_devices(from_mut(&mut person.node), own, &held, 1_000_000, &SEAL)
            .await
            .unwrap();
    }
}

async fn receive_existing(
    participants: [DeviceSession<'_>; 2],
    own: &dyapp_profile::VerifiedProfile,
) {
    for (session, person) in participants {
        session
            .receive(
                from_mut(&mut person.node),
                &person.device_key,
                std::slice::from_ref(own),
                &SEAL,
            )
            .await
            .unwrap();
    }
}

async fn join_third(
    third: &mut Person,
    own: &dyapp_profile::VerifiedProfile,
    group: &[u8],
) -> Session {
    let mut session = session(third, "third-own-new");
    let held = [own.clone()];
    let events = session
        .receive(from_mut(&mut third.node), &third.device_key, &held, &SEAL)
        .await
        .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Joined(id)] if id == group),
        "{events:?}"
    );
    session
        .maintain_own_devices(from_mut(&mut third.node), own, &held, 1_000_000, &SEAL)
        .await
        .unwrap();
    session
}

#[tokio::test]
async fn a_third_own_device_reuses_the_private_group_and_syncs_both_directions() {
    let address = link_node().await;
    let (mut first, mut second, own) = Box::pin(own_device_pair(&address)).await;
    let mut first_session = session(&first, "third-own-first");
    let mut second_session = session(&second, "third-own-second");
    check_own_group_join(
        (&mut first_session, &mut first),
        (&mut second_session, &mut second),
        &own,
        1_000_000,
    )
    .await;
    let group = first_session.own_group().unwrap();
    let (mut third, own) = Box::pin(publish_third(&mut first, &second, &address)).await;
    maintain_existing(
        [
            (&mut first_session, &mut first),
            (&mut second_session, &mut second),
        ],
        &own,
    )
    .await;
    receive_existing(
        [
            (&mut first_session, &mut first),
            (&mut second_session, &mut second),
        ],
        &own,
    )
    .await;
    let mut third_session = join_third(&mut third, &own, &group).await;
    let held = [own];
    sync_own_private_state(
        (&mut first_session, &mut first),
        (&mut third_session, &mut third),
        &held,
    )
    .await;
    for session in [&first_session, &second_session, &third_session] {
        assert_eq!(session.own_group().as_ref(), Some(&group));
        assert!(session.store().chat_rows().unwrap().is_empty());
    }
}
