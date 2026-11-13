//! Real mailbox closure and profile directory writes must not expose pending fields.
use super::*;

async fn linked_with_both_owner_bindings() -> HiddenLink {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    let mut remote = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    remote.device_owners = vec![dyapp_client::owner_binding(
        &f.owner.device_key,
        &f.owner.identity.public_key(),
    )];
    remote.version = remote.version.checked_add(1).unwrap();
    let owner = &mut f.owner;
    publish_profile_change(&mut owner.node, &owner.identity, &mut remote)
        .await
        .unwrap();
    f.finish().await;
    f.owner.node.fetch(&f.owner.device_key).await.unwrap();
    f
}

async fn revoke_phone(f: &mut HiddenLink, store: &Store, pending: &Profile) -> Profile {
    publish::revoke_device(
        store,
        &mut f.laptop.node,
        &f.laptop.identity,
        pending,
        &f.owner.device_key.public_key(),
    )
    .await
    .unwrap()
}

fn check_private(pending: &Profile, current: &Profile, before: &Profile, accepted: &Profile) {
    assert_eq!(
        (&accepted.name, &accepted.interests, &accepted.photos),
        (&before.name, &before.interests, &before.photos)
    );
    let expected = Profile {
        version: current.version,
        devices: accepted.devices.clone(),
        device_owners: accepted.device_owners.clone(),
        previous: accepted.previous.clone(),
        ..pending.clone()
    };
    assert_eq!(*current, expected);
}

#[tokio::test]
async fn revoking_a_real_mailbox_preserves_pending_profile_data_and_can_repeat() {
    let mut f = Box::pin(linked_with_both_owner_bindings()).await;
    let store = f.store();
    let before = store.profile_draft().unwrap().unwrap();
    let pending = directory::pending_profile(&store, &before, &f.laptop.identity);
    let public = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    let dropped = revoke_phone(&mut f, &store, &pending).await;
    let accepted = f
        .owner
        .node
        .get_profile(&f.owner.identity.peer_id())
        .await
        .unwrap()
        .profile;
    check_private(&pending, &dropped, &public, &accepted);
    assert!(f.owner.node.fetch(&f.owner.device_key).await.is_err());
    assert!(f.laptop.node.fetch(&f.laptop.device_key).await.is_ok());
    let hash = pending.photos[0].thumbnail.as_slice().try_into().unwrap();
    assert!(matches!(
        f.laptop.node.get_media(hash).await,
        Err(dyapp_client::Error::Status(Status::NotFound))
    ));
    assert!(store
        .profile_photo(&pending.photos[0].thumbnail)
        .unwrap()
        .is_some());
    let repeated = revoke_phone(&mut f, &store, &pending).await;
    assert_eq!(repeated, dropped);
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        accepted
    );
}
