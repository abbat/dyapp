//! Mailbox callbacks count irreversible requests instead of inferring them from timing.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn draft(identity: &Identity, key: &Identity, version: u64) -> Profile {
    let device = dyapp_messaging::mls::Device::new(identity, key.public_key(), 1).unwrap();
    Profile {
        version,
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(key, &identity.public_key())],
        ..Default::default()
    }
}

#[tokio::test]
async fn exhausted_version_never_closes_the_mailbox() {
    let identity = Identity::from_secret(&[7; 32]);
    let key = Identity::from_secret(&[8; 32]);
    let before = draft(&identity, &key, u64::MAX);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.save_profile_draft(&before).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let (mut node, held) = node::observed_revokes(
        Vec::new(),
        |_, _| {},
        |_| {},
        move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Status::Ok
        },
    )
    .await;
    *held.lock().unwrap() = Some(before.sign(&identity));
    let result =
        crate::publish::revoke_device(&store, &mut node, &identity, &before, &key.public_key())
            .await;
    assert!(matches!(
        result,
        Err(Failure::Store(crate::store::Error::ProfileVersionExhausted))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(store.profile_draft().unwrap(), Some(before));
}

#[tokio::test]
async fn refused_mailbox_keeps_binding_and_can_be_retried() {
    let identity = Identity::from_secret(&[7; 32]);
    let key = Identity::from_secret(&[8; 32]);
    let before = draft(&identity, &key, 1);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.save_profile_draft(&before).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let (mut node, held) = node::observed_revokes(
        Vec::new(),
        |_, _| {},
        |_| {},
        move || {
            if observed.fetch_add(1, Ordering::SeqCst) == 0 {
                Status::Refused
            } else {
                Status::Ok
            }
        },
    )
    .await;
    *held.lock().unwrap() = Some(before.sign(&identity));
    let failed =
        crate::publish::revoke_device(&store, &mut node, &identity, &before, &key.public_key())
            .await;
    assert!(matches!(
        failed,
        Err(Failure::Node(crate::Error::Status(Status::Refused)))
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(before.clone()));
    assert_eq!(held.lock().unwrap().as_ref(), Some(&before.sign(&identity)));
    let retried =
        crate::publish::revoke_device(&store, &mut node, &identity, &before, &key.public_key())
            .await
            .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(retried.devices, [] as [SignedRecord; 0]);
    assert_eq!(retried.device_owners, [] as [SignedRecord; 0]);
}
