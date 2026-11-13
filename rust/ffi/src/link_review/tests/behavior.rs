use super::*;
use std::fs;

#[test]
fn common_review_releases_leases_and_saves_only_the_original_slots_local_draft() {
    for layout in [MobileSlotLayout::Android, MobileSlotLayout::Ios] {
        let f = Fixture::new(layout);
        let review = f.worker.prepare().unwrap();
        let snapshot = review.snapshot();
        assert_eq!(snapshot.differences, [LinkReviewField::Name]);
        assert_eq!(
            (
                snapshot.local.fields.name.as_str(),
                snapshot.remote.fields.name.as_str()
            ),
            ("local", "remote")
        );
        assert!(snapshot.hidden);
        // An idle review permits a real registry mutation, without retargeting its original slot.
        let other = f.registry.create().unwrap();
        let saved = review.confirm(Fixture::choices()).unwrap();
        assert_eq!(
            (
                saved.published.fields.name.as_str(),
                saved.draft.fields.name.as_str()
            ),
            ("remote", "local")
        );
        assert_eq!((saved.published.version, saved.draft.version), (3, 5));
        assert!(saved.hidden && saved.pending);
        assert_eq!(
            f.store().link_pending().unwrap(),
            Some(b"original link".to_vec())
        );
        assert!(!PathBuf::from(other.slot.directory)
            .join("client.db")
            .exists());
        assert!(
            matches!(f.registry.resolve().unwrap(), crate::MobileSlotResolution::Ready { key, .. } if key.peer_id() == other.key.peer_id())
        );
        assert!(matches!(
            review.confirm(Fixture::choices()),
            Err(LinkReviewError::ReviewRequired)
        ));
    }
}

#[test]
fn response_boundary_edits_remain_intact_and_require_new_review() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let path = f.directory.join("client.db");
    let secret = f.slot.key.identity.secret();
    *f.server.on_read.lock().unwrap() = Some(Box::new(move || {
        let identity = dyapp_identity::Identity::from_secret(&secret);
        Store::open_existing(&path, &identity)
            .unwrap()
            .update_profile_draft(|draft| {
                draft.name = "typed during confirmation".into();
                Ok(())
            })
            .unwrap();
    }));
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(
        f.store().profile_draft().unwrap().unwrap().name,
        "typed during confirmation"
    );
}

#[test]
fn replacing_a_device_with_another_consistent_sealed_session_cannot_reuse_the_review() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let admitted = f.worker.owner.open().unwrap();
    let operation = &admitted.operation;
    let device = dyapp_identity::Identity::generate();
    let sealed = dyapp_messaging::mls::Device::new(operation.identity(), device.public_key(), 1)
        .unwrap()
        .seal(operation.state_key())
        .unwrap();
    let wrapped: Vec<_> = device.secret().iter().map(|byte| byte ^ 0x5a).collect();
    fs::write(f.directory.join("device.key"), wrapped).unwrap();
    let mut store = f.store();
    store.stage_outbound(&sealed, &[]).unwrap();
    drop(store);
    drop(admitted);
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
}

#[test]
fn replacing_only_the_sealing_key_cannot_reuse_the_original_certificate_review() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let admitted = f.worker.owner.open().unwrap();
    let operation = &admitted.operation;
    let state = [23_u8; 32];
    let snapshot = admitted.session.store().sealed_device().unwrap().unwrap();
    let device = dyapp_messaging::mls::Device::open(&snapshot, operation.state_key()).unwrap();
    assert_eq!(device.certificate(), &review.certificate);
    let sealed = device.seal(&state).unwrap();
    fs::write(
        f.directory.join("chat-state.key"),
        state.map(|byte| byte ^ 0x5a),
    )
    .unwrap();
    let mut store = f.store();
    store.stage_outbound(&sealed, &[]).unwrap();
    drop(store);
    drop(admitted);
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
}

#[test]
fn dropping_the_ui_owner_keeps_admission_until_the_native_confirmation_finishes() {
    use std::sync::mpsc;
    use std::time::Duration;
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let (entered, ready) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    *f.server.on_read.lock().unwrap() = Some(Box::new(move || {
        entered.send(()).unwrap();
        resumed.recv_timeout(Duration::from_secs(15)).unwrap();
    }));
    let native = review.clone();
    let (finished, completed) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let result = native.confirm(Fixture::choices());
        drop(native);
        finished.send(result).unwrap();
    });
    ready.recv_timeout(Duration::from_secs(15)).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.root.join("profiles.lock"))
        .unwrap();
    assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
    drop(review);
    assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
    resume.send(()).unwrap();
    assert!(
        completed
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            .unwrap()
            .pending
    );
    thread.join().unwrap();
    lock.try_lock().unwrap();
}

#[test]
fn replaced_link_and_active_network_are_refused_before_a_local_save() {
    for replace_link in [true, false] {
        let f = Fixture::new(MobileSlotLayout::Android);
        let review = f.worker.prepare().unwrap();
        if replace_link {
            f.store().set_link_pending(b"replacement link").unwrap();
        } else {
            let network = Network {
                name: "other".into(),
                ..f.server.network.clone()
            };
            dyapp_identity::network::save(
                &f.directory,
                dyapp_identity::network::Settings::Active,
                &network,
            )
            .unwrap();
        }
        assert!(matches!(
            review.confirm(Fixture::choices()),
            Err(LinkReviewError::ReviewRequired)
        ));
        assert!(matches!(
            review.resume_link(),
            Err(LinkReviewError::ReviewRequired)
        ));
        assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
    }
}

#[test]
fn a_non_ready_slot_is_refused_without_saving() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    // Simulate lost inventory reversibly; do not delete the retained fixture.
    fs::rename(f.directory.join("state"), f.directory.join("state.saved")).unwrap();
    fs::write(f.directory.join("state"), b"pending\n").unwrap();
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::Failed { .. })
    ));
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::Failed { .. })
    ));
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
}

#[test]
fn a_changed_remote_profile_keeps_the_typed_review_refusal() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let remote = Profile {
        version: 4,
        name: "other device".into(),
        ..review.review.remote_profile().clone()
    };
    *f.server.record.lock().unwrap() = remote.sign(&f.slot.key.identity);
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::ReviewRequired)
    ));
    assert_eq!(f.store().profile_draft().unwrap().unwrap().name, "local");
}

#[test]
fn an_unlisted_original_device_cannot_restore_its_certificate() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let remote = Profile {
        version: 3,
        name: "remote".into(),
        ..Default::default()
    };
    *f.server.record.lock().unwrap() = remote.sign(&f.slot.key.identity);
    let review = f.worker.prepare().unwrap();
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::DeviceNotListed)
    ));
    assert_eq!(f.store().device_listed().unwrap(), 0);
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
}

#[test]
fn a_missing_database_is_not_recreated_by_an_old_review() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    let path = f.directory.join("client.db");
    fs::rename(&path, f.directory.join("client.db.saved")).unwrap();
    assert!(matches!(
        review.confirm(Fixture::choices()),
        Err(LinkReviewError::Failed { .. })
    ));
    assert!(matches!(
        review.resume_link(),
        Err(LinkReviewError::Failed { .. })
    ));
    assert!(!path.exists());
    assert!(f.directory.join("client.db.saved").is_file());
}

#[test]
fn invalid_choices_keep_the_typed_selection_error_and_do_not_save() {
    let f = Fixture::new(MobileSlotLayout::Android);
    let review = f.worker.prepare().unwrap();
    assert!(matches!(
        review.confirm(Vec::new()),
        Err(LinkReviewError::Selection { .. })
    ));
    let mut duplicate = Fixture::choices();
    duplicate.extend(Fixture::choices());
    assert!(matches!(
        review.confirm(duplicate),
        Err(LinkReviewError::Selection { .. })
    ));
    assert_eq!(f.store().profile_draft().unwrap().unwrap().version, 4);
}
