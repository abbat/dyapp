//! Revoking a device is not approval to publish editable pending values.
use super::*;

struct Fixture {
    identity: Identity,
    key: Identity,
    path: std::path::PathBuf,
    store: Store,
    remote: Profile,
    pending: Profile,
}
impl Fixture {
    fn new(hidden: bool, remote_version: u64) -> Self {
        let identity = Identity::from_secret(&[7; 32]);
        let key = Identity::from_secret(&[8; 32]);
        let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
        let path = temp_db_path();
        let store = Store::open(&path, &identity).unwrap();
        let remote = Profile {
            version: remote_version,
            name: "public".into(),
            devices: vec![device.certificate().clone()],
            device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
            ..Default::default()
        };
        let hash = store.save_profile_blob(&[0xFF, 0xD8, 0xFF, 9]).unwrap();
        let mut pending = Profile {
            version: if hidden { 2 } else { 1 },
            name: "private pending".into(),
            interests: vec!["private interest".into()],
            photos: vec![dyapp_profile::Photo {
                thumbnail: hash.clone(),
                full: vec![hash],
            }],
            ..remote.clone()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !crate::publish::work(&mut pending, &identity.public_key(), 1, 8) {
            assert!(
                std::time::Instant::now() < deadline,
                "one-bit proof did not complete"
            );
        }
        store.save_profile_draft(&pending).unwrap();
        store.set_profile_hidden(hidden).unwrap();
        Self {
            identity,
            key,
            path,
            store,
            remote,
            pending,
        }
    }

    async fn revoke(&self, node: &mut Node) -> Result<Profile, Failure> {
        crate::publish::revoke_device(
            &self.store,
            node,
            &self.identity,
            &self.pending,
            &self.key.public_key(),
        )
        .await
    }

    fn check_pending(&self, current: &Profile, published: &Profile) {
        let expected = Profile {
            version: current.version,
            devices: published.devices.clone(),
            device_owners: published.device_owners.clone(),
            previous: Some(
                dyapp_profile::ProfileCondition::observed(Some(&self.remote.sign(&self.identity)))
                    .unwrap(),
            ),
            ..self.pending.clone()
        };
        assert_eq!(*current, expected);
        assert!(self
            .store
            .profile_photo(&current.photos[0].thumbnail)
            .unwrap()
            .is_some());
    }
}

#[tokio::test]
async fn an_older_node_cannot_restore_a_device_revoked_in_a_known_newer_ordinary_profile() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut f = Fixture::new(true, 1);
    let other = Identity::from_secret(&[9; 32]);
    let device = dyapp_messaging::mls::Device::new(&f.identity, other.public_key(), 1).unwrap();
    crate::publish::list_device(
        &mut f.remote,
        device.certificate().clone(),
        crate::owner_binding(&other, &f.identity.public_key()),
    );
    f.pending.version = 5;
    f.store.save_profile_draft(&f.pending).unwrap();
    f.store.own_profile_age(3, 10).unwrap();
    f.store.note_device_listed(3).unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let observed = counter.clone();
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
    let obsolete = f.remote.sign(&f.identity);
    *held.lock().unwrap() = Some(obsolete.clone());
    assert!(matches!(
        f.revoke(&mut node).await,
        Err(Failure::DirectoryBehindKnown {
            known_version: 3,
            ..
        })
    ));
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    assert_eq!(held.lock().unwrap().as_ref(), Some(&obsolete));
    assert_eq!(f.store.profile_draft().unwrap(), Some(f.pending));
}

#[tokio::test]
async fn a_missing_public_profile_does_not_republish_a_private_draft_or_close_the_mailbox() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let f = Fixture::new(false, 1);
    let mailbox_count = Arc::new(AtomicUsize::new(0));
    let observed = mailbox_count.clone();
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
    assert!(matches!(
        f.revoke(&mut node).await,
        Err(Failure::Node(crate::Error::Reply(_)))
    ));
    assert_eq!(mailbox_count.load(Ordering::SeqCst), 0);
    assert!(held.lock().unwrap().is_none());
    assert_eq!(f.store.profile_draft().unwrap(), Some(f.pending));
}

#[tokio::test]
async fn a_partial_revoke_retries_without_publishing_another_profile_version() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let f = Fixture::new(false, 1);
    let submission_count = Arc::new(AtomicUsize::new(0));
    let mailbox_count = Arc::new(AtomicUsize::new(0));
    let on_write = submission_count.clone();
    let on_close = mailbox_count.clone();
    let path = f.path.clone();
    let copy = Identity::from_secret(&f.identity.secret());
    let (mut node, held) = node::observed_revokes(
        Vec::new(),
        move |request, _| {
            if matches!(
                request,
                Request::Publish(_) | Request::ConditionalPublish(_)
            ) {
                on_write.fetch_add(1, Ordering::SeqCst);
                Store::open_existing(&path, &copy)
                    .unwrap()
                    .set_profile_hidden(true)
                    .unwrap();
            }
        },
        |_| {},
        move || {
            on_close.fetch_add(1, Ordering::SeqCst);
            Status::Ok
        },
    )
    .await;
    *held.lock().unwrap() = Some(f.remote.sign(&f.identity));
    assert!(matches!(
        f.revoke(&mut node).await,
        Err(Failure::Store(
            crate::store::Error::ProfileCompletionSuperseded
        ))
    ));
    let accepted = held.lock().unwrap().clone().unwrap();
    let published = dyapp_profile::verify(&accepted).unwrap().profile;
    assert_eq!(published.devices, [] as [SignedRecord; 0]);
    let current = f.revoke(&mut node).await.unwrap();
    f.check_pending(&current, &published);
    assert_eq!(held.lock().unwrap().as_ref(), Some(&accepted));
    assert_eq!(f.revoke(&mut node).await.unwrap(), current);
    assert_eq!(held.lock().unwrap().as_ref(), Some(&accepted));
    assert_eq!(submission_count.load(Ordering::SeqCst), 1);
    assert_eq!(mailbox_count.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn revocation_keeps_existing_pending_values_private_for_visible_hidden_and_newer_sources() {
    for hidden in [false, true] {
        for version in [1, 3] {
            Box::pin(check_private(hidden, version)).await;
        }
    }
}
async fn check_private(hidden: bool, version: u64) {
    let f = Fixture::new(hidden, version);
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(f.remote.sign(&f.identity));
    let current = f.revoke(&mut node).await.unwrap();
    let remote = dyapp_profile::verify(held.lock().unwrap().as_ref().unwrap())
        .unwrap()
        .profile;
    assert_eq!(remote.name, f.remote.name);
    assert_eq!(remote.interests, f.remote.interests);
    assert_eq!(remote.photos, f.remote.photos);
    assert_eq!(remote.devices, [] as [SignedRecord; 0]);
    assert_eq!(remote.device_owners, [] as [SignedRecord; 0]);
    f.check_pending(&current, &remote);
    assert_eq!(f.store.profile_hidden().unwrap(), hidden);
}
