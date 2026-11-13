//! Recovery uses actual request callbacks and the original admitted mobile slot.
use super::*;
use dyapp_p2p_net::proto;
use prost::Message as _;
use std::sync::atomic::Ordering;

fn confirmed(f: &Fixture, source: LinkReviewSource) -> Arc<ProfileAdoptionReview> {
    let store = f.store();
    store
        .set_link_pending(&store.link_state().unwrap())
        .unwrap();
    let review = f.worker.prepare().unwrap();
    review
        .confirm(vec![LinkReviewChoice {
            field: LinkReviewField::Name,
            source,
        }])
        .unwrap();
    assert_eq!(
        store.device_listed().unwrap(),
        review.review.remote_profile().version
    );
    review
}

fn check_original_packages(f: &Fixture, review: &ProfileAdoptionReview) {
    let admitted = f.worker.owner.open().unwrap();
    assert_eq!(admitted.operation.binding(), review.admission);
    assert_eq!(admitted.session.certificate().unwrap(), review.certificate);
    let device = admitted.operation.device().public_key();
    let signer = admitted.operation.device().peer_id();
    let owner = dyapp_identity::sha256(&admitted.operation.identity().public_key()).to_vec();
    let mailbox = dyapp_client::mailbox(&device).to_vec();
    let fallback = admitted.session.store().last_resort().unwrap().unwrap();
    assert!(fallback.published);
    drop(admitted);
    let records = f.server.packages.records.lock().unwrap().clone();
    let mut ordinary = 0;
    let mut reusable = 0;
    for record in &records {
        assert_eq!(
            record.verify(dyapp_identity::Domain::KeyPackage).unwrap(),
            signer
        );
        let package = proto::KeyPackageRecord::decode(record.payload.as_slice()).unwrap();
        assert_eq!((&package.owner, &package.mailbox), (&owner, &mailbox));
        if package.last_resort {
            reusable += 1;
        } else {
            ordinary += 1;
        }
    }
    assert_eq!(ordinary, crate::chats::KEY_PACKAGES);
    assert_eq!(reusable, 1);
    assert!(records.contains(&fallback.record));
}

#[test]
fn common_resume_completes_the_original_slot_and_idle_retry_does_not_connect() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let mut f = Fixture::new(layout);
        let review = confirmed(&f, LinkReviewSource::Local);
        f.store()
            .update_profile_draft(|draft| {
                draft.name = "edited after confirmation".into();
                Ok(())
            })
            .unwrap();
        let before = f.store().profile_draft().unwrap();
        let other = f.registry.create().unwrap();
        assert!(review.resume_link().unwrap());
        assert_eq!(f.store().profile_draft().unwrap(), before);
        assert!(f.store().link_pending().unwrap().is_none());
        assert!(!PathBuf::from(other.slot.directory)
            .join("client.db")
            .exists());
        check_original_packages(&f, &review);
        let sealed = f.store().sealed_device().unwrap();
        f.server.shutdown();
        assert!(!review.resume_link().unwrap());
        assert_eq!(f.store().sealed_device().unwrap(), sealed);
        assert_eq!(f.store().profile_draft().unwrap(), before);
    }
}

#[test]
fn refused_fallback_retains_pending_and_retries_the_exact_saved_record() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = confirmed(&f, LinkReviewSource::Local);
    let pending = f.store().link_pending().unwrap();
    let draft = f.store().profile_draft().unwrap();
    f.server
        .packages
        .fallback_status
        .store(proto::Status::Refused.into(), Ordering::SeqCst);
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::Failed { .. })
    ));
    assert_eq!(f.store().link_pending().unwrap(), pending);
    assert_eq!(f.store().profile_draft().unwrap(), draft);
    let before = f.store().last_resort().unwrap().unwrap();
    assert!(!before.published);
    assert_eq!(
        f.server.packages.records.lock().unwrap().len(),
        crate::chats::KEY_PACKAGES
    );
    f.store().set_blocked("later private choice", true).unwrap();
    f.server
        .packages
        .fallback_status
        .store(proto::Status::Ok.into(), Ordering::SeqCst);
    assert!(review.resume_link().unwrap());
    let after = f.store().last_resort().unwrap().unwrap();
    assert_eq!(
        (after.record, after.revision, after.rotate_at),
        (before.record, before.revision, before.rotate_at)
    );
    assert!(after.published);
    assert!(f.store().blocked("later private choice").unwrap());
    assert!(f.store().link_pending().unwrap().is_none());
    check_original_packages(&f, &review);
}

#[test]
fn replacement_during_package_reply_survives_old_completion_and_requires_new_review() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = confirmed(&f, LinkReviewSource::Local);
    let mut replacement = f.store().link_state().unwrap();
    replacement.extend([0xF8, 0x07, 0x01]);
    let copy = replacement.clone();
    let directory = f.directory.clone();
    let key = f.slot.key.clone();
    *f.server.packages.on_write.lock().unwrap() = Some(Box::new(move || {
        Store::open_existing(&directory.join("client.db"), &key.identity)
            .unwrap()
            .set_link_pending(&copy)
            .unwrap();
    }));
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().link_pending().unwrap(), Some(replacement));
    let accepted = f.server.packages.records.lock().unwrap().len();
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.server.packages.records.lock().unwrap().len(), accepted);
    let fresh = f.worker.prepare().unwrap();
    fresh.confirm(Fixture::choices()).unwrap();
    assert!(fresh.resume_link().unwrap());
    assert!(f.store().link_pending().unwrap().is_none());
}

#[test]
fn resumption_cannot_bypass_an_unconfirmed_profile_field_decision() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let store = f.store();
    store
        .set_link_pending(&store.link_state().unwrap())
        .unwrap();
    let before = store.profile_draft().unwrap();
    let review = f.worker.prepare().unwrap();
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().profile_draft().unwrap(), before);
    assert_eq!(
        f.store().link_pending().unwrap().as_deref(),
        Some(review.review.pending_link_state())
    );
    assert_eq!(f.server.packages.records.lock().unwrap().len(), 0);
}

#[test]
fn changed_public_fields_need_fresh_owner_review_before_resumption() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = confirmed(&f, LinkReviewSource::Local);
    let before = f.store().profile_draft().unwrap().unwrap();
    let remote = Profile {
        version: before.version.checked_add(1).unwrap(),
        name: "another public value".into(),
        ..review.review.remote_profile().clone()
    };
    *f.server.record.lock().unwrap() = remote.sign(&f.slot.key.identity);
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().profile_draft().unwrap(), Some(before));
    assert_eq!(
        f.store().link_pending().unwrap().as_deref(),
        Some(review.review.pending_link_state())
    );
    assert_eq!(f.server.packages.records.lock().unwrap().len(), 0);
}

#[test]
fn verified_revocation_has_a_distinct_native_error_and_cannot_restore_its_device() {
    let f = Fixture::new(MobileSlotLayout::Android);
    assert_eq!(f.store().device_listed().unwrap(), 0);
    let review = confirmed(&f, LinkReviewSource::Remote);
    let remote = Profile {
        version: f
            .store()
            .profile_draft()
            .unwrap()
            .unwrap()
            .version
            .checked_add(1)
            .unwrap(),
        devices: Vec::new(),
        device_owners: Vec::new(),
        ..review.review.remote_profile().clone()
    };
    *f.server.record.lock().unwrap() = remote.sign(&f.slot.key.identity);
    let result = review.resume_link();
    assert_eq!(
        f.server.profile_writes.load(Ordering::SeqCst),
        0,
        "a revoked device must not be relisted"
    );
    assert!(
        matches!(result, Err(LinkReviewError::Revoked)),
        "unexpected resumption result: {result:?}"
    );
    assert_eq!(f.server.packages.records.lock().unwrap().len(), 0);
    assert!(f.store().link_pending().unwrap().is_some());
    let admitted = f.worker.owner.open().unwrap();
    assert_eq!(admitted.session.certificate().unwrap(), review.certificate);
    assert_eq!(admitted.operation.binding(), review.admission);
}

#[test]
fn an_unconfirmed_comparison_still_protects_verified_device_authorization() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        for advance in [0, 1] {
            let f = Fixture::new(layout);
            // Equal editable fields need no owner field decision. Preparation itself stays read-only.
            let remote = f.worker.prepare().unwrap().review.remote_profile().clone();
            let store = f.store();
            store.save_profile_draft(&remote).unwrap();
            store
                .set_link_pending(&store.link_state().unwrap())
                .unwrap();
            drop(store);
            let review = f.worker.prepare().unwrap();
            assert_eq!(
                review.review.differences(),
                [] as [publish::AdoptionField; 0]
            );
            assert_eq!(f.store().device_listed().unwrap(), 0);
            let revoked = Profile {
                version: remote.version.checked_add(advance).unwrap(),
                devices: Vec::new(),
                device_owners: Vec::new(),
                ..remote.clone()
            };
            *f.server.record.lock().unwrap() = revoked.sign(&f.slot.key.identity);
            let result = review.resume_link();
            if advance == 0 {
                // The unchanged hidden search head can refuse the contradictory import first.
                assert!(
                    matches!(
                        result,
                        Err(LinkReviewError::Revoked | LinkReviewError::ReviewRequired)
                    ),
                    "{result:?}"
                );
            } else {
                assert!(
                    matches!(result, Err(LinkReviewError::Revoked)),
                    "{result:?}"
                );
            }
            assert_eq!(f.store().device_listed().unwrap(), remote.version);
            assert_eq!(f.server.profile_writes.load(Ordering::SeqCst), 0);
            assert!(f.server.packages.records.lock().unwrap().is_empty());
            assert!(f.store().link_pending().unwrap().is_some());
        }
    }
}

#[test]
fn nested_private_import_refusal_remains_distinct_from_backend_failure() {
    let error = dyapp_client::session::Error::Publish(publish::Failure::Store(
        dyapp_client::store::Error::LinkStateReviewRequired,
    ));
    assert!(matches!(
        LinkReviewError::from(error),
        LinkReviewError::PrivateStateReviewRequired
    ));
}

#[test]
fn dropping_the_ui_handle_keeps_admission_until_actual_package_completion() {
    use std::sync::mpsc;
    use std::time::Duration;
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = confirmed(&f, LinkReviewSource::Local);
    let (entered, ready) = mpsc::channel();
    let (release, released) = mpsc::channel();
    *f.server.packages.on_write.lock().unwrap() = Some(Box::new(move || {
        entered.send(()).unwrap();
        released.recv_timeout(Duration::from_secs(15)).unwrap();
    }));
    let native = review.clone();
    let (finished, completed) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = native.resume_link();
        drop(native);
        finished.send(result).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(15)).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.root.join("profiles.lock"))
        .unwrap();
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    drop(review);
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    release.send(()).unwrap();
    assert!(completed
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap());
    thread.join().unwrap();
    lock.try_lock().unwrap();
}
