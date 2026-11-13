//! Actual readback callbacks distinguish editor consent from network device authority.
use super::publication_tests::{at_readback, edit_at_readback};
use super::*;
use crate::session::{Error as SessionError, Session};
use dyapp_messaging::mls::Device;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

type ProfileCell = Arc<Mutex<Option<SignedRecord>>>;

struct Subject {
    identity: Identity,
    device_key: Identity,
    session: Session,
    authority: Profile,
    path: std::path::PathBuf,
}
impl Subject {
    fn new(known_listing: bool) -> Self {
        let identity = Identity::from_secret(&[7; 32]);
        let device_key = Identity::from_secret(&[8; 32]);
        let device = Device::new(&identity, device_key.public_key(), 1).unwrap();
        let other_key = Identity::from_secret(&[9; 32]);
        let other = Device::new(&identity, other_key.public_key(), 2).unwrap();
        let mut baseline = Profile {
            version: 3,
            name: "prepared".into(),
            ..Default::default()
        };
        list_device(
            &mut baseline,
            device.certificate().clone(),
            crate::owner_binding(&device_key, &identity.public_key()),
        );
        list_device(
            &mut baseline,
            other.certificate().clone(),
            crate::owner_binding(&other_key, &identity.public_key()),
        );
        let mut authority = baseline.clone();
        authority.version = 2;
        authority.name = "public".into();
        unlist_device(&mut authority, &other_key.public_key()).unwrap();
        let path = temp_db_path();
        let mut store = Store::open(&path, &identity).unwrap();
        store.save_profile_draft(&baseline).unwrap();
        store.set_profile_hidden(true).unwrap();
        store
            .hold_search_profile(&Profile::tombstone(baseline.version, 0, 0).sign(&identity))
            .unwrap();
        store
            .stage_outbound(&device.seal(&[3; 32]).unwrap(), &[])
            .unwrap();
        if known_listing {
            store.note_device_listed(1).unwrap();
        }
        Self {
            identity,
            device_key,
            session: Session::open(store, &[3; 32]).unwrap(),
            authority,
            path,
        }
    }
}

#[tokio::test]
async fn ordinary_fields_keep_the_original_editor_baseline_and_verified_directory() {
    let f = Subject::new(true);
    let mut attempt = f
        .session
        .store()
        .prepare_profile_publication(&f.identity)
        .unwrap();
    attempt.draft_mut().name = "explicit request".into();
    let (mut node, held) = profile_node(Vec::new()).await;
    *held.lock().unwrap() = Some(f.authority.sign(&f.identity));
    let result = f
        .session
        .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
        .await
        .unwrap();
    assert_eq!(result.published.name, "explicit request");
    assert_eq!(result.draft.name, "explicit request");
    assert_eq!(result.published.devices, f.authority.devices);
    assert_eq!(result.published.device_owners, f.authority.device_owners);
    assert!(!result.pending && !result.hidden);
    assert_eq!(
        f.session.store().device_listed().unwrap(),
        result.published.version
    );
}

#[tokio::test]
async fn actual_accepted_readback_keeps_later_private_fields_photos_and_proof() {
    let f = Subject::new(true);
    let attempt = f
        .session
        .store()
        .prepare_profile_publication(&f.identity)
        .unwrap();
    let path = f.path.clone();
    let (mut node, held) = profile_node_observed(
        Vec::new(),
        at_readback(edit_at_readback(
            path,
            Identity::from_secret(&f.identity.secret()),
        )),
    )
    .await;
    *held.lock().unwrap() = Some(f.authority.sign(&f.identity));
    let result = f
        .session
        .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
        .await
        .unwrap();
    assert_eq!(result.published.name, "prepared");
    assert_eq!(result.published.photos.len(), 0);
    assert_eq!(result.published.devices, f.authority.devices);
    assert_eq!(result.draft.name, "during readback");
    assert_eq!(result.draft.photos.len(), 1);
    assert!(result.draft.pow_bits > result.published.pow_bits);
    assert!(result.pending && !result.hidden);
    assert!(f
        .session
        .store()
        .profile_photo(&result.draft.photos[0].thumbnail)
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn source_change_before_the_guard_cannot_restore_an_observed_directory() {
    let f = Subject::new(true);
    let before = f.session.store().profile_draft().unwrap();
    let attempt = f
        .session
        .store()
        .prepare_profile_publication(&f.identity)
        .unwrap();
    let current = Arc::new(OnceLock::<ProfileCell>::new());
    let callback_cell = current.clone();
    let mut changed = false;
    let replacement = Profile {
        version: f.authority.version.checked_add(1).unwrap(),
        devices: Vec::new(),
        device_owners: Vec::new(),
        ..f.authority.clone()
    };
    let signed = replacement.sign(&f.identity);
    let first = f.authority.sign(&f.identity);
    let writes = Arc::new(AtomicUsize::new(0));
    let observed_writes = writes.clone();
    let (mut node, held) = profile_node_observed(Vec::new(), move |request, reply| {
        if matches!(request, Request::Publish(_)) {
            observed_writes.fetch_add(1, Ordering::SeqCst);
        }
        if !changed && matches!(request, Request::Get(_)) && reply.record.as_ref() == Some(&first) {
            changed = true;
            *callback_cell.get().unwrap().lock().unwrap() = Some(signed.clone());
        }
    })
    .await;
    assert!(current.set(held.clone()).is_ok());
    *held.lock().unwrap() = Some(f.authority.sign(&f.identity));
    let result = f
        .session
        .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
        .await;
    assert!(
        matches!(
            result,
            Err(SessionError::Revoked | SessionError::Publish(Failure::Missed))
        ),
        "{result:?}"
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0);
    assert_eq!(
        held.lock().unwrap().as_ref(),
        Some(&replacement.sign(&f.identity))
    );
    assert_eq!(f.session.store().profile_draft().unwrap(), before);
    assert!(f.session.store().device_listed().unwrap() <= f.authority.version);
}

#[tokio::test]
async fn wrong_keys_and_pending_enrollment_refuse_before_any_profile_request() {
    let f = Subject::new(true);
    let reads = Arc::new(AtomicUsize::new(0));
    let observed = reads.clone();
    let (mut node, held) = profile_node_observed(Vec::new(), move |_, _| {
        observed.fetch_add(1, Ordering::SeqCst);
    })
    .await;
    let wrong = Identity::from_secret(&[4; 32]);
    for owner_wrong in [true, false] {
        let attempt = f
            .session
            .store()
            .prepare_profile_publication(&f.identity)
            .unwrap();
        let identity = if owner_wrong { &wrong } else { &f.identity };
        let device = if owner_wrong { &f.device_key } else { &wrong };
        assert!(matches!(
            f.session
                .publish_profile(&mut node, identity, device, attempt)
                .await,
            Err(SessionError::Store(crate::store::Error::WrongIdentity))
        ));
    }
    let pending = f.session.store().link_state().unwrap();
    f.session.store().set_link_pending(&pending).unwrap();
    let before = f.session.store().profile_draft().unwrap();
    let attempt = f
        .session
        .store()
        .prepare_profile_publication(&f.identity)
        .unwrap();
    assert!(matches!(
        f.session
            .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
            .await,
        Err(SessionError::Publish(Failure::LinkPending))
    ));
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    assert!(held.lock().unwrap().is_none());
    assert_eq!(f.session.store().profile_draft().unwrap(), before);
    assert_eq!(f.session.store().link_pending().unwrap(), Some(pending));
}

#[tokio::test]
async fn missing_withdrawn_or_unproven_membership_cannot_become_first_enrollment() {
    for case in 0..3 {
        let f = Subject::new(case != 2);
        let before = f.session.store().profile_draft().unwrap();
        let (mut node, held) = profile_node(Vec::new()).await;
        let source = match case {
            0 => None,
            1 => Some(Profile::tombstone(f.authority.version, 0, 0)),
            _ => Some(Profile {
                devices: Vec::new(),
                device_owners: Vec::new(),
                ..f.authority.clone()
            }),
        };
        *held.lock().unwrap() = source.as_ref().map(|profile| profile.sign(&f.identity));
        let original = held.lock().unwrap().clone();
        let attempt = f
            .session
            .store()
            .prepare_profile_publication(&f.identity)
            .unwrap();
        assert!(matches!(
            f.session
                .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
                .await,
            Err(SessionError::Publish(Failure::DirectoryAuthorityRequired))
        ));
        assert_eq!(*held.lock().unwrap(), original);
        assert_eq!(f.session.store().profile_draft().unwrap(), before);
    }
}

#[tokio::test]
async fn legacy_draft_derived_listing_knowledge_is_not_silently_decremented() {
    let f = Subject::new(true);
    let before = f.session.store().profile_draft().unwrap().unwrap();
    let mut legacy = before.clone();
    // Reproduce the old client call before connecting, rather than editing a stored counter.
    f.session
        .list_self(
            &mut legacy,
            crate::owner_binding(&f.device_key, &f.identity.public_key()),
        )
        .unwrap();
    let poisoned = f.session.store().device_listed().unwrap();
    assert_eq!(poisoned, before.version);
    assert!(poisoned > f.authority.version);
    let (mut node, held) = profile_node(Vec::new()).await;
    let actual = f.authority.sign(&f.identity);
    *held.lock().unwrap() = Some(actual.clone());
    let attempt = f
        .session
        .store()
        .prepare_profile_publication(&f.identity)
        .unwrap();
    let result = f
        .session
        .publish_profile(&mut node, &f.identity, &f.device_key, attempt)
        .await;
    assert!(
        matches!(
            result,
            Err(SessionError::Publish(Failure::Node(Error::Reply(_))))
        ),
        "{result:?}"
    );
    assert_eq!(f.session.store().device_listed().unwrap(), poisoned);
    assert_eq!(f.session.store().profile_draft().unwrap(), Some(before));
    assert_eq!(*held.lock().unwrap(), Some(actual));
}
