//! A later public readback cannot replace a private draft saved before a recovery retry.
use super::*;

async fn initial_adoption(f: &mut HiddenLink, listed: bool) -> Profile {
    if listed {
        return f.finish().await;
    }
    let store = f.store();
    store.set_link_pending(&f.state).unwrap();
    drop(Session::open_or_create(store, &f.laptop.identity, &f.laptop.device_key, &SEAL).unwrap());
    publish::adopt(
        &mut f.store(),
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.state,
        1_000_000,
    )
    .await
    .unwrap()
    .published
}

async fn prepare(hidden: bool, listed: bool, revoked: bool) -> (HiddenLink, Profile, Profile) {
    let mut f = Box::pin(HiddenLink::with_roles(vec![Role::Store, Role::Media])).await;
    if !hidden {
        let phone = Store::open(&store_path("visible-advanced-phone"), &f.owner.identity).unwrap();
        f.state = phone.link_state().unwrap();
    }
    let accepted = Box::pin(initial_adoption(&mut f, listed)).await;
    let store = f.store();
    let baseline = store.profile_draft().unwrap().unwrap();
    let pending = directory::pending_profile(&store, &baseline, &f.laptop.identity);
    store.set_link_pending(&f.state).unwrap();
    let mut remote = Profile {
        version: pending.version.checked_add(1).unwrap(),
        name: "new public name".into(),
        ..accepted
    };
    if revoked {
        let session = Session::open(f.store(), &SEAL).unwrap();
        let certificate = session.certificate().unwrap();
        remote.devices.retain(|value| value != &certificate);
        remote
            .device_owners
            .retain(|value| value.public_key != f.laptop.device_key.public_key());
    }
    let owner = &mut f.owner;
    publish_profile_change(&mut owner.node, &owner.identity, &mut remote)
        .await
        .unwrap();
    (f, pending, remote)
}

async fn refused_retry(
    f: &mut HiddenLink,
    pending: &Profile,
    remote: &Profile,
) -> publish::AdoptionReview {
    let store = f.store();
    let head = store.due_search_profile(1_000_000).unwrap();
    let seen = store.own_profile_seen().unwrap();
    let result = Session::finish_link(
        store,
        &mut f.laptop.node,
        &f.laptop.identity,
        &f.laptop.device_key,
        2,
        &SEAL,
        1_000_000,
    )
    .await;
    assert!(matches!(
        result,
        Err(dyapp_client::session::Error::Publish(
            publish::Failure::AdoptionReviewRequired
        ))
    ));
    let store = f.store();
    assert_eq!(store.profile_draft().unwrap().as_ref(), Some(pending));
    assert!(store
        .profile_photo(&pending.photos[0].thumbnail)
        .unwrap()
        .is_some());
    assert_eq!(store.link_pending().unwrap(), Some(f.state.clone()));
    assert_eq!(store.due_search_profile(1_000_000).unwrap(), head);
    assert_eq!(store.own_profile_seen().unwrap(), seen);
    let review = publish::review_adoption(&store, &mut f.laptop.node, &f.laptop.identity)
        .await
        .unwrap();
    assert_eq!(review.local_profile(), pending);
    assert_eq!(review.remote_profile(), remote);
    review
}

async fn choose_local(f: &mut HiddenLink, review: &publish::AdoptionReview) -> bool {
    let choices: Vec<_> = review
        .differences()
        .into_iter()
        .map(|field| (field, publish::AdoptionSource::Local))
        .collect();
    let preview = review.select_fields(&choices).unwrap();
    assert_eq!(preview.devices, review.remote_profile().devices);
    assert_eq!(preview.device_owners, review.remote_profile().device_owners);
    let session = Session::open(f.store(), &SEAL).unwrap();
    let listed = review
        .remote_profile()
        .devices
        .contains(&session.certificate().unwrap());
    let result = publish::confirm_adoption_review(
        &session,
        &mut f.laptop.node,
        &f.laptop.identity,
        review,
        &choices,
    )
    .await;
    if listed {
        let saved = result.unwrap();
        assert!(saved.pending);
        let expected = Profile {
            version: saved.draft.version,
            devices: saved.published.devices.clone(),
            device_owners: saved.published.device_owners.clone(),
            previous: saved.published.previous.clone(),
            ..review.local_profile().clone()
        };
        assert_eq!(saved.draft, expected);
    } else {
        assert!(matches!(
            result,
            Err(publish::AdoptionConfirmError::DeviceNotListed)
        ));
        assert_eq!(
            session.store().profile_draft().unwrap().as_ref(),
            Some(review.local_profile())
        );
    }
    assert!(session.store().link_pending().unwrap().is_some());
    listed
}

async fn check_advanced(hidden: bool, listed: bool, revoked: bool) {
    let (mut f, pending, remote) = Box::pin(prepare(hidden, listed, revoked)).await;
    let review = Box::pin(refused_retry(&mut f, &pending, &remote)).await;
    if Box::pin(choose_local(&mut f, &review)).await {
        assert_eq!(f.finish().await, remote);
        assert!(f.store().link_pending().unwrap().is_none());
    }
    assert_eq!(
        f.owner
            .node
            .get_profile(&f.owner.identity.peer_id())
            .await
            .unwrap()
            .profile,
        remote
    );
    let hash = &pending.photos[0].thumbnail;
    assert!(f.store().profile_photo(hash).unwrap().is_some());
    assert!(matches!(
        f.owner
            .node
            .get_media(hash.as_slice().try_into().unwrap())
            .await,
        Err(dyapp_client::Error::Status(Status::NotFound))
    ));
}

#[tokio::test]
async fn advanced_link_readback_requires_review_without_discarding_pending_fields() {
    for hidden in [false, true] {
        for listed in [false, true] {
            Box::pin(check_advanced(hidden, listed, false)).await;
        }
    }
}

#[tokio::test]
async fn advanced_link_review_cannot_restore_a_revoked_device() {
    for hidden in [false, true] {
        Box::pin(check_advanced(hidden, true, true)).await;
    }
}
