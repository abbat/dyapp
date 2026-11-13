use super::*;
use crate::store::tests::temp_db_path;

fn owner() -> Identity {
    Identity::from_secret(&[7; 32])
}

fn accepted(attempt: &ProfilePublication) -> Profile {
    Profile {
        version: attempt.draft().version.checked_add(1).unwrap(),
        ..attempt.draft().clone()
    }
}

#[test]
fn unchanged_baseline_accepts_unsaved_request_and_keeps_exact_output() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.save_profile_draft(&Profile::default()).unwrap();
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.draft_mut().name = "request".into();
    let published = accepted(&attempt);
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(outcome.published, published);
    assert_eq!(outcome.draft, published);
    assert!(!outcome.pending);
    assert!(!outcome.hidden);
}

#[test]
fn newer_edit_is_pending_not_fabricated_as_published() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let base = Profile {
        name: "base".into(),
        profession: "base job".into(),
        ..Default::default()
    };
    store.save_profile_draft(&base).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let current = Profile {
        name: "current".into(),
        profession: "current job".into(),
        company: "current employer".into(),
        interests: vec!["rust".into()],
        ..base.clone()
    };
    store.save_profile_draft(&current).unwrap();
    let published = accepted(&attempt);
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(outcome.published, published);
    assert_eq!(
        outcome.draft,
        Profile {
            version: published.version,
            ..current
        }
    );
    assert!(outcome.pending);
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(published.sign(&identity))
    );
}

#[test]
fn coupled_groups_and_removed_devices_do_not_mix_or_resurrect() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let base = Profile {
        country: "US".into(),
        place: "5128581".into(),
        birth_year: Some(2000),
        birth_month: Some(1),
        birth_day: Some(1),
        income_from: Some(10),
        income_to: Some(20),
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        ..Default::default()
    };
    store.save_profile_draft(&base).unwrap();
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.draft_mut().country = "FR".into();
    attempt.draft_mut().place = "2988507".into();
    attempt.draft_mut().birth_year = Some(2001);
    attempt.draft_mut().income_to = Some(30);
    attempt.draft_mut().seeking_genders = vec![1];
    let mut current = base;
    current.place.clear();
    current.birth_day = Some(2);
    current.income_from = Some(15);
    current.not_looking = true;
    current.devices.clear();
    current.device_owners.clear();
    store.save_profile_draft(&current).unwrap();
    let published = accepted(&attempt);
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(
        outcome.draft,
        Profile {
            version: published.version,
            ..current
        }
    );
    assert_eq!(outcome.published.devices.len(), 1);
    assert_eq!(outcome.draft.devices, Vec::new());
    assert_eq!(outcome.draft.device_owners, Vec::new());
}

#[test]
fn current_photo_bytes_and_published_refs_survive_atomic_pruning() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 3], &[0xFF, 0xD8, 0xFF, 4])
        .unwrap();
    store.move_profile_photo(1, 0).unwrap();
    let current = store.profile_draft().unwrap().unwrap();
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(outcome.draft.photos, current.photos);
    for photo in outcome.draft.photos.iter().chain(&published.photos) {
        for hash in photo.full.iter().chain([&photo.thumbnail]) {
            assert!(store.profile_photo(hash).unwrap().is_some());
        }
    }
    assert!(outcome.pending);
}

#[test]
fn stronger_proof_survives_without_running_solver_in_completion() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    let work = store.prepare_profile_work(&identity, 1, 1024).unwrap();
    assert!(store.commit_profile_work(&work).unwrap().solved);
    let current = store.profile_draft().unwrap().unwrap();
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(
        (outcome.draft.pow_nonce, outcome.draft.pow_bits),
        (current.pow_nonce, current.pow_bits)
    );
    assert_eq!(outcome.published.pow_bits, 0);
    assert!(outcome.pending);
}

#[test]
fn first_draft_proof_progress_does_not_erase_in_memory_device_enrichment() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    crate::publish::list_device(
        attempt.draft_mut(),
        device.certificate().clone(),
        crate::owner_binding(&key, &identity.public_key()),
    );
    let work = store.prepare_profile_work(&identity, 1, 1024).unwrap();
    assert!(store.commit_profile_work(&work).unwrap().solved);
    let published = accepted(&attempt);
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    assert_eq!(outcome.draft.devices, published.devices);
    assert_eq!(outcome.draft.device_owners, published.device_owners);
    assert_eq!(outcome.draft.pow_bits, 1);
    assert!(outcome.pending);
}

#[test]
fn newer_hide_supersedes_publication_without_removing_its_tombstone() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    let hidden = store
        .hide_profile_draft(&identity, attempt.draft())
        .unwrap();
    let tombstone = store.due_search_profile(0).unwrap();
    assert!(matches!(
        store.commit_profile_publication(&identity, &attempt, &published, false),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(hidden));
    assert!(store.profile_hidden().unwrap());
    assert_eq!(store.due_search_profile(0).unwrap(), tombstone);
}

#[test]
fn older_completion_leaves_current_projection_unchanged() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let old = store.prepare_profile_publication(&identity).unwrap();
    let first = accepted(&old);
    store
        .commit_profile_publication(&identity, &old, &first, false)
        .unwrap();
    let mut newer = store.prepare_profile_publication(&identity).unwrap();
    newer.draft_mut().name = "newer".into();
    let second = accepted(&newer);
    store
        .commit_profile_publication(&identity, &newer, &second, false)
        .unwrap();
    assert!(matches!(
        store.commit_profile_publication(&identity, &old, &first, false),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(second.clone()));
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(second.sign(&identity))
    );
}

#[test]
fn hidden_relist_commits_tombstone_and_pending_edits_in_one_transaction() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_profile_hidden(true).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    store
        .save_profile_draft(&Profile {
            name: "pending".into(),
            ..Default::default()
        })
        .unwrap();
    let outcome = store
        .commit_profile_publication(&identity, &attempt, &published, true)
        .unwrap();
    assert!(outcome.hidden && outcome.pending);
    assert_eq!(outcome.draft.version, published.version + 1);
    assert_eq!(outcome.draft.name, "pending");
    assert_eq!(
        store.due_search_profile(0).unwrap(),
        Some(
            Profile::search_tombstone(
                outcome.draft.version,
                outcome.draft.pow_nonce,
                outcome.draft.pow_bits
            )
            .sign(&identity)
        )
    );
}

#[test]
fn final_prune_failure_rolls_back_draft_search_and_visibility() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    store.set_profile_hidden(true).unwrap();
    let mut attempt = store.prepare_profile_publication(&identity).unwrap();
    attempt.draft_mut().photos.clear();
    let before = store.profile_draft().unwrap();
    let published = accepted(&attempt);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_publication_prune` BEFORE DELETE ON `settings`
        WHEN OLD.`key` GLOB 'profile_photo:*'
        BEGIN SELECT RAISE(ABORT, 'publication prune refused'); END;",
        )
        .unwrap();
    let error = store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap_err();
    assert!(error.to_string().contains("publication prune refused"));
    assert_eq!(store.profile_draft().unwrap(), before);
    assert!(store.profile_hidden().unwrap());
    assert_eq!(store.known_search_version().unwrap(), 0);
    assert!(store.due_search_profile(0).unwrap().is_none());
}

#[test]
fn a_same_version_fork_does_not_replace_the_first_completion() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let first = store.prepare_profile_publication(&identity).unwrap();
    let mut fork = store.prepare_profile_publication(&identity).unwrap();
    fork.draft_mut().name = "fork".into();
    let published = accepted(&first);
    store
        .commit_profile_publication(&identity, &first, &published, false)
        .unwrap();
    assert!(matches!(
        store.commit_profile_publication(&identity, &fork, &accepted(&fork), false),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(published.clone()));
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(published.sign(&identity))
    );
}

#[test]
fn writer_admission_precedes_reading_current_draft() {
    let identity = owner();
    let path = temp_db_path();
    let first = Store::open(&path, &identity).unwrap();
    let second = Store::open(&path, &identity).unwrap();
    second.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let attempt = second.prepare_profile_publication(&identity).unwrap();
    let writer = Transaction::new_unchecked(&first.db, TransactionBehavior::Immediate).unwrap();
    assert!(
        matches!(second.commit_profile_publication(&identity, &attempt, &accepted(&attempt), false),
        Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _)))
            if error.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    writer.rollback().unwrap();
    assert!(second.profile_draft().unwrap().is_none());
}

#[test]
fn post_network_snapshot_keeps_exact_output_and_observes_later_hide_and_edits() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    store
        .commit_profile_publication(&identity, &attempt, &published, false)
        .unwrap();
    store.hide_profile_draft(&identity, &published).unwrap();
    let hidden = store
        .profile_publication_state(&identity, &published)
        .unwrap();
    assert!(hidden.hidden);
    assert!(!hidden.pending);
    assert!(hidden.draft.version > published.version);
    store
        .update_profile_draft(|draft| {
            draft.name = "later".into();
            Ok(())
        })
        .unwrap();
    let current = store
        .profile_publication_state(&identity, &published)
        .unwrap();
    assert_eq!(current.published, published);
    assert_eq!(current.draft.name, "later");
    assert!(current.pending && current.hidden);
}

#[test]
fn current_publication_snapshot_refuses_missing_draft_or_wrong_owner() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let published = Profile {
        version: 1,
        ..Default::default()
    };
    assert!(matches!(
        store.profile_publication_state(&identity, &published),
        Err(Error::ProfileCompletionSuperseded)
    ));
    assert!(matches!(
        store.profile_publication_state(&Identity::from_secret(&[9; 32]), &published),
        Err(Error::WrongIdentity)
    ));
}

#[test]
fn prepared_owner_and_exact_readback_are_checked_before_projection() {
    let identity = owner();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let published = accepted(&attempt);
    let stranger = Identity::from_secret(&[9; 32]);
    assert!(matches!(
        store.prepare_profile_publication(&stranger),
        Err(Error::WrongIdentity)
    ));
    assert!(matches!(
        store.commit_profile_publication(&stranger, &attempt, &published, false),
        Err(Error::WrongIdentity)
    ));
    let other = Store::open(&temp_db_path(), &stranger).unwrap();
    let foreign = other.prepare_profile_publication(&stranger).unwrap();
    assert!(matches!(
        store.commit_profile_publication(&identity, &foreign, &accepted(&foreign), false),
        Err(Error::WrongIdentity)
    ));
    let wrong = Profile {
        name: "not the request".into(),
        ..published
    };
    assert!(matches!(
        store.commit_profile_publication(&identity, &attempt, &wrong, false),
        Err(Error::InvalidSetting)
    ));
    assert!(store.profile_draft().unwrap().is_none());
}
