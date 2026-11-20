//! The shared desktop/native maintenance entry point drives actual private-device traffic.
use super::*;

mod third;

#[tokio::test]
async fn common_maintenance_starts_one_group_and_syncs_private_changes() {
    let address = link_node().await;
    let (mut first, mut second, own) = Box::pin(own_device_pair(&address)).await;
    let held = [own.clone()];
    let now = 1_000_000;
    let mut first_session = session(&first, "maintenance-first");
    let mut second_session = session(&second, "maintenance-second");
    second_session
        .maintain_own_devices(from_mut(&mut second.node), &own, &held, now, &SEAL)
        .await
        .unwrap();
    assert!(second_session.own_group().is_none());
    first_session
        .maintain_own_devices(from_mut(&mut first.node), &own, &held, now, &SEAL)
        .await
        .unwrap();
    let group = first_session.own_group().unwrap();
    let events = second_session
        .receive(from_mut(&mut second.node), &second.device_key, &held, &SEAL)
        .await
        .unwrap();
    assert!(
        matches!(events.as_slice(), [Event::Joined(id)] if *id == group),
        "{events:?}"
    );
    second_session
        .maintain_own_devices(from_mut(&mut second.node), &own, &held, now, &SEAL)
        .await
        .unwrap();
    sync_own_private_state(
        (&mut first_session, &mut first),
        (&mut second_session, &mut second),
        &held,
    )
    .await;
    assert_eq!(first_session.own_group(), Some(group.clone()));
    assert_eq!(second_session.own_group(), Some(group));
}
