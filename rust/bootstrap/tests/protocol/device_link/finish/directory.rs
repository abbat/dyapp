//! Directory publication is not consent to publish the pending local profile.
use super::*;

pub(super) fn pending_profile(store: &Store, adopted: &Profile, identity: &Identity) -> Profile {
    let hash = store.save_profile_blob(&[0xFF, 0xD8, 0xFF, 7]).unwrap();
    let mut pending = Profile {
        name: "private pending name".into(),
        interests: vec!["private pending interest".into()],
        photos: vec![dyapp_profile::Photo {
            thumbnail: hash.clone(),
            full: vec![hash],
        }],
        ..adopted.clone()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !publish::work(&mut pending, &identity.public_key(), 1, 8) {
        assert!(
            std::time::Instant::now() < deadline,
            "one-bit proof did not complete"
        );
    }
    store.save_profile_draft(&pending).unwrap();
    pending
}

#[tokio::test]
async fn finishing_an_already_revoked_link_does_not_restore_its_device() {
    let mut f = Box::pin(HiddenLink::new()).await;
    let listed = f.prepare_legacy_partial_link().await;
    let mut revoked = Profile {
        version: listed.version.checked_add(1).unwrap(),
        devices: vec![f.owner.device.certificate().clone()],
        device_owners: Vec::new(),
        ..listed
    };
    let owner = &mut f.owner;
    publish_profile_change(&mut owner.node, &owner.identity, &mut revoked)
        .await
        .unwrap();
    let result = Session::finish_link(
        f.store(),
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.laptop.device_key,
        2,
        &SEAL,
        1_000_000,
    )
    .await;
    assert!(matches!(result, Err(dyapp_client::session::Error::Revoked)));
    assert!(f.store().link_pending().unwrap().is_some());
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        revoked
    );
}

#[tokio::test]
async fn finishing_an_unlisted_link_does_not_publish_retained_pending_profile_fields() {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    let mut store = f.store();
    store.set_link_pending(&f.state).unwrap();
    let adopted = publish::adopt(
        &mut store,
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.state,
        1_000_000,
    )
    .await
    .unwrap();
    let pending = pending_profile(&store, &adopted.draft, &f.laptop.identity);
    drop(store);
    let published = f.finish().await;
    assert_eq!(published.name, adopted.published.name);
    assert_eq!(published.interests, adopted.published.interests);
    assert_eq!(published.photos, adopted.published.photos);
    f.check_listed(&published);
    let local = f.store().profile_draft().unwrap().unwrap();
    let expected = Profile {
        version: local.version,
        devices: published.devices.clone(),
        device_owners: published.device_owners.clone(),
        previous: published.previous.clone(),
        ..pending
    };
    assert_eq!(local, expected);
    let hash = &local.photos[0].thumbnail;
    assert!(f.store().profile_photo(hash).unwrap().is_some());
    assert!(matches!(
        f.owner
            .node
            .get_media(hash.as_slice().try_into().unwrap())
            .await,
        Err(dyapp_client::Error::Status(Status::NotFound))
    ));
    assert_eq!(f.finish().await, published);
    assert_eq!(f.store().profile_draft().unwrap(), Some(local));
}
