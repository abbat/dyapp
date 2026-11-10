use super::*;
use crate::identity_slots::remove::Stage;
use crate::identity_slots::{active, Active, State};

#[test]
fn every_precompletion_fault_retains_discoverable_state_and_can_resume() {
    for (layout, storage) in [
        (Layout::Android, Storage::Wrapped),
        (Layout::Ios, Storage::Wrapped),
        (Layout::Desktop, Storage::System),
        (Layout::Desktop, Storage::File),
    ] {
        for stage in [
            Stage::Prepared,
            Stage::Marked,
            Stage::SelectionCleared,
            Stage::CredentialsErased,
            Stage::Quarantined,
            Stage::DataCleared,
        ] {
            let (root, registry, backend) = fixture(layout, storage);
            let peer = backend.identity.peer_id();
            assert!(registry
                .remove_with(FIRST, Some(&peer), &backend, |current| {
                    if current == stage {
                        Err("injected stop".into())
                    } else {
                        Ok(())
                    }
                })
                .is_err());
            let reopened = Registry::new(root.clone(), layout);
            let listed = reopened.list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(
                listed[0].state,
                if stage == Stage::Prepared {
                    State::Ready
                } else {
                    State::Deleting
                },
                "{layout:?}/{storage:?}/{stage:?}"
            );
            reopened.remove(FIRST, Some(&peer), &backend).unwrap();
            assert_eq!(reopened.list().unwrap(), [] as [Slot; 0]);
            assert!(!root
                .join("profiles")
                .join(format!("{FIRST}.deleting"))
                .exists());
        }
    }
}

#[test]
fn ancillary_erase_failure_preserves_identity_for_exact_retry_confirmation() {
    let (_, registry, backend) = fixture(Layout::Desktop, Storage::System);
    let peer = backend.identity.peer_id();
    backend.fail.set(Some(Credential::Device));
    assert!(registry.remove(FIRST, Some(&peer), &backend).is_err());
    assert_eq!(*backend.present.borrow(), [false, true, true]);
    assert_eq!(
        *backend.erased.borrow(),
        [Credential::Seal, Credential::Device]
    );
    assert!(registry
        .remove(FIRST, Some("wrong peer"), &backend)
        .is_err());
    assert_eq!(*backend.present.borrow(), [false, true, true]);
    backend.fail.set(None);
    registry.remove(FIRST, Some(&peer), &backend).unwrap();
    assert_eq!(
        *backend.erased.borrow(),
        [
            Credential::Seal,
            Credential::Device,
            Credential::Device,
            Credential::Identity
        ]
    );
}

#[test]
fn a_backend_that_reports_success_without_erasing_remains_deleting() {
    let (_, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    let peer = backend.identity.peer_id();
    backend.lie.set(true);
    assert!(registry.remove(FIRST, Some(&peer), &backend).is_err());
    assert_eq!(registry.list().unwrap()[0].state, State::Deleting);
    assert!(registry.remove(FIRST, None, &backend).is_err());
    backend.lie.set(false);
    registry.remove(FIRST, Some(&peer), &backend).unwrap();
}

#[test]
fn reported_absence_does_not_override_an_identity_that_still_opens() {
    for (layout, storage) in [
        (Layout::Android, Storage::Wrapped),
        (Layout::Desktop, Storage::System),
    ] {
        let (root, registry, backend) = fixture(layout, storage);
        let data = root.join("profiles").join(FIRST).join("client.db");
        fs::write(&data, b"preserve until key erasure is verified").unwrap();
        backend.stale_open.set(true);
        assert!(registry
            .remove(FIRST, Some(&backend.identity.peer_id()), &backend)
            .is_err());
        assert_eq!(
            fs::read(&data).unwrap(),
            b"preserve until key erasure is verified"
        );
        assert!(registry.remove(FIRST, None, &backend).is_err());
        backend.stale_open.set(false);
        registry.remove(FIRST, None, &backend).unwrap();
    }
}

#[test]
fn postcompletion_failure_reports_committed_removal_with_unconfirmed_persistence() {
    let (root, registry, backend) = fixture(Layout::Android, Storage::Wrapped);
    let outcome = registry
        .remove_with(
            FIRST,
            Some(&backend.identity.peer_id()),
            &backend,
            |stage| {
                if stage == Stage::Committed {
                    assert!(!root
                        .join("profiles")
                        .join(format!(".{FIRST}.removed"))
                        .exists());
                    Err("sync failed after removal".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
    assert_eq!(
        outcome.durability_detail.as_deref(),
        Some("sync failed after removal")
    );
    assert_eq!(registry.list().unwrap(), [] as [Slot; 0]);
    assert_eq!(active(&root, Layout::Android), Active::Absent);
}

#[test]
fn hidden_cleanup_with_unknown_storage_never_guesses_credentials_or_key_bytes() {
    for credentials_present in [false, true] {
        let (root, registry, backend) = fixture(Layout::Desktop, Storage::System);
        registry
            .remove_with(
                FIRST,
                Some(&backend.identity.peer_id()),
                &backend,
                |stage| {
                    if stage == Stage::DataCleared {
                        Err("stop".into())
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
        let hidden = root.join("profiles").join(format!(".{FIRST}.removed"));
        if credentials_present {
            backend.present.borrow_mut()[0] = true;
        } else {
            fs::write(hidden.join("identity.key"), b"unverified identity").unwrap();
        }
        assert!(registry.remove(FIRST, None, &backend).is_err());
        assert!(hidden.is_dir());
        assert_eq!(backend.present.borrow()[0], credentials_present);
    }
}

#[test]
fn apple_tombstone_without_a_directory_resumes_only_with_absent_credentials() {
    let (root, registry, backend) = fixture(Layout::Ios, Storage::Wrapped);
    registry
        .remove_with(
            FIRST,
            Some(&backend.identity.peer_id()),
            &backend,
            |stage| {
                if stage == Stage::DataCleared {
                    Err("stop".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();
    fs::remove_dir(root.join("profiles").join(FIRST)).unwrap();
    registry.remove(FIRST, None, &backend).unwrap();
    assert_eq!(registry.list().unwrap(), [] as [Slot; 0]);
}
