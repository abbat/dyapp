//! Device-only consent preserves pending fields and binds the exact public source.
use super::*;

fn local(store: &Store, remote: &Profile) -> Profile {
    let hash = store.save_profile_blob(&[0xFF, 0xD8, 0xFF, 8]).unwrap();
    let pending = Profile {
        version: remote.version.checked_add(2).unwrap(),
        name: "pending local".into(),
        interests: vec!["private interest".into()],
        photos: vec![dyapp_profile::Photo {
            thumbnail: hash.clone(),
            full: vec![hash],
        }],
        ..remote.clone()
    };
    store.save_profile_draft(&pending).unwrap();
    store.set_profile_hidden(true).unwrap();
    pending
}

#[tokio::test]
async fn directory_only_completion_keeps_existing_pending_fields_and_photo_bytes() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let remote = Profile {
        version: 3,
        name: "public".into(),
        ..Default::default()
    };
    let pending = local(&store, &remote);
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.directory_from(&remote);
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(remote.sign(&identity));
    let outcome = crate::publish::relist_prepared(&store, &mut node, &identity, &attempt)
        .await
        .unwrap();
    assert_eq!(outcome.published.name, remote.name);
    assert_eq!(outcome.published.photos, [] as [dyapp_profile::Photo; 0]);
    let expected = Profile {
        version: outcome.draft.version,
        previous: Some(
            dyapp_profile::ProfileCondition::observed(Some(&remote.sign(&identity))).unwrap(),
        ),
        ..pending
    };
    assert_eq!(outcome.draft, expected);
    assert!(outcome.hidden && outcome.pending);
    assert!(store
        .profile_photo(&outcome.draft.photos[0].thumbnail)
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn changed_remote_authorization_below_the_local_version_floor_is_not_overwritten() {
    let identity = Identity::from_secret(&[7; 32]);
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let remote = Profile {
        version: 3,
        name: "public".into(),
        devices: vec![device.certificate().clone()],
        ..Default::default()
    };
    let pending = local(&store, &remote);
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.directory_from(&remote);
    let revoked = Profile {
        version: remote.version.checked_add(1).unwrap(),
        devices: Vec::new(),
        ..remote
    };
    assert!(revoked.version < pending.version);
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(revoked.sign(&identity));
    assert!(matches!(
        crate::publish::relist_prepared(&store, &mut node, &identity, &attempt).await,
        Err(Failure::Missed)
    ));
    assert_eq!(
        held.lock().unwrap().as_ref(),
        Some(&revoked.sign(&identity))
    );
    assert_eq!(store.profile_draft().unwrap(), Some(pending));
}

#[tokio::test]
async fn directory_completion_also_preserves_a_later_response_boundary_edit() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    let remote = Profile {
        version: 3,
        name: "public".into(),
        ..Default::default()
    };
    local(&store, &remote);
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.directory_from(&remote);
    let copy = Identity::from_secret(&identity.secret());
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if matches!(request, Request::Get(_)) {
            Store::open_existing(&path, &copy)
                .unwrap()
                .update_profile_draft(|draft| {
                    draft.name = "typed later".into();
                    Ok(())
                })
                .unwrap();
        }
    })
    .await;
    *held.lock().unwrap() = Some(remote.sign(&identity));
    let outcome = crate::publish::relist_prepared(&store, &mut node, &identity, &attempt)
        .await
        .unwrap();
    assert_eq!(outcome.published.name, "public");
    assert_eq!(outcome.draft.name, "typed later");
    assert!(outcome.pending);
}
