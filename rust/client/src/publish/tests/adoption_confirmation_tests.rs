//! Phase-armed response callbacks complete local mutations before confirmation resumes.
use super::*;
use crate::publish::{AdoptionConfirmError, AdoptionField, AdoptionReview, AdoptionSource};
use crate::session::Session;
use std::sync::atomic::{AtomicBool, Ordering};

struct Fixture {
    identity: Identity,
    session: Session,
    node: Node,
    remote: Profile,
    held: Arc<Mutex<Option<SignedRecord>>>,
    review: AdoptionReview,
}

fn prepare_local(store: &Store, identity: &Identity, remote: &Profile, hidden: bool) {
    let local = Profile {
        version: if hidden { 4 } else { 1 },
        name: "local".into(),
        ..remote.clone()
    };
    store.save_profile_draft(&local).unwrap();
    store.set_profile_hidden(hidden).unwrap();
    store.set_link_pending(b"pending state").unwrap();
    store.note_device_listed(remote.version).unwrap();
    if hidden {
        store
            .hold_search_profile(&Profile::tombstone(local.version, 0, 0).sign(identity))
            .unwrap();
    }
}

impl Fixture {
    async fn new(
        hidden: bool,
        mut change: impl FnMut(&mut Store, &Identity) + Send + 'static,
    ) -> Self {
        let identity = Identity::from_secret(&[7; 32]);
        let device = Identity::from_secret(&[8; 32]);
        let path = temp_db_path();
        let session = Session::open_or_create(
            Store::open(&path, &identity).unwrap(),
            &identity,
            &device,
            &[6; 32],
        )
        .unwrap();
        let remote = Profile {
            version: 3,
            name: "remote".into(),
            devices: vec![session.certificate().unwrap()],
            device_owners: vec![crate::owner_binding(&device, &identity.public_key())],
            ..Default::default()
        };
        prepare_local(session.store(), &identity, &remote, hidden);
        let armed = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&armed);
        let copy = Identity::from_secret(&identity.secret());
        let (mut node, held) = profile_node_observed(Vec::new(), move |request, _| {
            if matches!(request, Request::Get(_)) && signal.swap(false, Ordering::SeqCst) {
                change(&mut Store::open(&path, &copy).unwrap(), &copy);
            }
        })
        .await;
        *held.lock().unwrap() = Some(remote.sign(&identity));
        let review = Box::pin(crate::publish::review_adoption(
            session.store(),
            &mut node,
            &identity,
        ))
        .await
        .unwrap();
        armed.store(true, Ordering::SeqCst);
        Self {
            identity,
            session,
            node,
            remote,
            held,
            review,
        }
    }

    async fn confirm(
        &mut self,
    ) -> Result<crate::store::ProfilePublicationOutcome, AdoptionConfirmError> {
        crate::publish::confirm_adoption_review(
            &self.session,
            &mut self.node,
            &self.identity,
            &self.review,
            &[(AdoptionField::Name, AdoptionSource::Local)],
        )
        .await
    }
}

#[tokio::test]
async fn reviewed_hidden_fields_stay_local_and_the_same_confirmation_cannot_spend_another_version()
{
    let mut f = Fixture::new(true, |_, _| {}).await;
    let before = f.session.store().profile_draft().unwrap().unwrap();
    let outcome = f.confirm().await.unwrap();
    assert_eq!(outcome.published, f.remote);
    assert_eq!(outcome.draft.name, "local");
    assert_eq!(
        outcome.draft.version,
        before.version.checked_add(1).unwrap()
    );
    assert!(outcome.pending && outcome.hidden);
    assert_eq!(outcome.draft.devices, f.remote.devices);
    assert_eq!(
        f.held.lock().unwrap().as_ref(),
        Some(&f.remote.sign(&f.identity))
    );
    let saved = f.session.store().profile_draft().unwrap();
    assert!(matches!(
        f.confirm().await,
        Err(AdoptionConfirmError::Recovery(
            Failure::AdoptionReviewRequired
        ))
    ));
    assert_eq!(f.session.store().profile_draft().unwrap(), saved);
    assert_eq!(
        f.session.store().link_pending().unwrap(),
        Some(b"pending state".to_vec())
    );
}

#[tokio::test]
async fn visible_review_queues_exact_remote_content_instead_of_pending_local_fields() {
    let mut f = Fixture::new(false, |_, _| {}).await;
    let outcome = f.confirm().await.unwrap();
    assert_eq!(outcome.draft.name, "local");
    assert_eq!(outcome.draft.version, f.remote.version);
    assert!(outcome.pending);
    assert!(!outcome.hidden);
    assert_eq!(
        f.session.store().due_search_profile(1000).unwrap(),
        Some(f.remote.sign(&f.identity))
    );
    assert_eq!(
        f.held.lock().unwrap().as_ref(),
        Some(&f.remote.sign(&f.identity))
    );
}

#[tokio::test]
async fn a_local_edit_completed_before_the_reply_is_not_overwritten() {
    let mut f = Fixture::new(true, |store, _| {
        store
            .update_profile_draft(|draft| {
                draft.name = "typed during confirmation".into();
                Ok(())
            })
            .unwrap();
    })
    .await;
    assert!(matches!(
        f.confirm().await,
        Err(AdoptionConfirmError::Recovery(
            Failure::AdoptionReviewRequired
        ))
    ));
    assert_eq!(
        f.session.store().profile_draft().unwrap().unwrap().name,
        "typed during confirmation"
    );
    assert_eq!(
        f.held.lock().unwrap().as_ref(),
        Some(&f.remote.sign(&f.identity))
    );
}

#[tokio::test]
async fn replacement_link_is_not_cleared_or_confirmed_using_the_old_context() {
    let mut f = Fixture::new(true, |store, _| {
        store.set_link_pending(b"replacement link").unwrap();
    })
    .await;
    let before = f.session.store().profile_draft().unwrap();
    assert!(matches!(
        f.confirm().await,
        Err(AdoptionConfirmError::Recovery(
            Failure::AdoptionReviewRequired
        ))
    ));
    assert_eq!(f.session.store().profile_draft().unwrap(), before);
    assert_eq!(
        f.session.store().link_pending().unwrap(),
        Some(b"replacement link".to_vec())
    );
}

#[tokio::test]
async fn a_changed_remote_profile_requires_review_without_discarding_local_values() {
    let mut f = Fixture::new(true, |_, _| {}).await;
    let before = f.session.store().profile_draft().unwrap();
    let remote = Profile {
        version: f.remote.version.checked_add(1).unwrap(),
        name: "other device".into(),
        ..f.remote.clone()
    };
    *f.held.lock().unwrap() = Some(remote.sign(&f.identity));
    assert!(matches!(
        f.confirm().await,
        Err(AdoptionConfirmError::Recovery(
            Failure::AdoptionReviewRequired
        ))
    ));
    assert_eq!(f.session.store().profile_draft().unwrap(), before);
    assert_eq!(
        f.held.lock().unwrap().as_ref(),
        Some(&remote.sign(&f.identity))
    );
}

#[tokio::test]
async fn ack_of_the_original_search_head_does_not_interrupt_owner_confirmation() {
    let mut f = Fixture::new(true, |store, identity| {
        let draft = store.profile_draft().unwrap().unwrap();
        store
            .search_profile_sent(
                &Profile::tombstone(draft.version, draft.pow_nonce, draft.pow_bits).sign(identity),
                100,
            )
            .unwrap();
    })
    .await;
    assert!(f.confirm().await.unwrap().pending);
}

#[tokio::test]
async fn an_unlisted_credential_is_not_automatically_reauthorized_by_review() {
    let mut f = Fixture::new(true, |_, _| {}).await;
    let before = f.session.store().profile_draft().unwrap();
    let remote = Profile {
        devices: Vec::new(),
        device_owners: Vec::new(),
        ..f.remote.clone()
    };
    *f.held.lock().unwrap() = Some(remote.sign(&f.identity));
    f.review = crate::publish::review_adoption(f.session.store(), &mut f.node, &f.identity)
        .await
        .unwrap();
    assert!(matches!(
        f.confirm().await,
        Err(AdoptionConfirmError::DeviceNotListed)
    ));
    assert_eq!(f.session.store().profile_draft().unwrap(), before);
    assert_eq!(
        f.held.lock().unwrap().as_ref(),
        Some(&remote.sign(&f.identity))
    );
}
