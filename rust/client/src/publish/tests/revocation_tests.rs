//! Profile-response callbacks establish local edits before the awaiting revocation proceeds.
use super::*;

#[tokio::test]
async fn revocation_preserves_an_edit_during_its_initial_read() {
    for hidden in [false, true] {
        for remote_version in [1, 3] {
            Box::pin(check_initial_read(hidden, remote_version)).await;
        }
    }
}

async fn check_initial_read(hidden: bool, remote_version: u64) {
    let identity = Identity::from_secret(&[7; 32]);
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let draft = Profile {
        version: if hidden { 2 } else { 1 },
        name: "before".into(),
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        ..Default::default()
    };
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    store.save_profile_draft(&draft).unwrap();
    store.set_profile_hidden(hidden).unwrap();
    let copy = Identity::from_secret(&identity.secret());
    let mut edited = false;
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if matches!(request, Request::Get(_)) && !edited {
            edited = true;
            Store::open(&path, &copy)
                .unwrap()
                .update_profile_draft(|current| {
                    current.name = "during initial read".into();
                    Ok(())
                })
                .unwrap();
        }
    })
    .await;
    *held.lock().unwrap() = Some(
        Profile {
            version: remote_version,
            ..draft.clone()
        }
        .sign(&identity),
    );
    let result =
        crate::publish::revoke_device(&store, &mut node, &identity, &draft, &key.public_key())
            .await
            .unwrap();
    let remote = dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(remote.profile.name, "before");
    assert_eq!(remote.profile.devices, [] as [SignedRecord; 0]);
    assert_eq!(result.name, "during initial read");
    assert_eq!(result.devices, [] as [SignedRecord; 0]);
    assert_eq!(store.profile_draft().unwrap(), Some(result));
    assert_eq!(store.profile_hidden().unwrap(), hidden);
}

#[tokio::test]
async fn foreign_profile_after_initial_read_does_not_replace_pending_local_edits() {
    type Held = Arc<Mutex<Option<SignedRecord>>>;
    let identity = Identity::from_secret(&[7; 32]);
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let draft = Profile {
        version: 1,
        name: "before".into(),
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        ..Default::default()
    };
    let newer = Profile {
        version: 3,
        name: "foreign".into(),
        ..draft.clone()
    }
    .sign(&identity);
    let path = temp_db_path();
    let store = Store::open(&path, &identity).unwrap();
    store.save_profile_draft(&draft).unwrap();
    let destination: Arc<Mutex<Option<Held>>> = Arc::new(Mutex::new(None));
    let target = Arc::clone(&destination);
    let copy = Identity::from_secret(&identity.secret());
    let mut changed = false;
    let replacement = newer.clone();
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
        if matches!(request, Request::Get(_)) && !changed {
            changed = true;
            Store::open(&path, &copy)
                .unwrap()
                .update_profile_draft(|local| {
                    local.name = "pending".into();
                    Ok(())
                })
                .unwrap();
            *target.lock().unwrap().as_ref().unwrap().lock().unwrap() = Some(replacement.clone());
        }
    })
    .await;
    *destination.lock().unwrap() = Some(Arc::clone(&held));
    *held.lock().unwrap() = Some(draft.sign(&identity));
    let result =
        crate::publish::revoke_device(&store, &mut node, &identity, &draft, &key.public_key())
            .await;
    assert!(matches!(result, Err(Failure::Missed)));
    assert_eq!(store.profile_draft().unwrap().unwrap().name, "pending");
    assert_eq!(held.lock().unwrap().as_ref(), Some(&newer));
}
