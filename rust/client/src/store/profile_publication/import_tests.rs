use super::*;
use crate::store::tests::temp_db_path;

#[test]
fn foreign_import_keeps_device_removal_as_one_directory_edit() {
    let (identity, store, _, mut imported) = fixture();
    let key = Identity::from_secret(&[8; 32]);
    let device = dyapp_messaging::mls::Device::new(&identity, key.public_key(), 1).unwrap();
    let baseline = Profile {
        devices: vec![device.certificate().clone()],
        device_owners: vec![crate::owner_binding(&key, &identity.public_key())],
        ..store.profile_draft().unwrap().unwrap()
    };
    store.save_profile_draft(&baseline).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    imported.devices = baseline.devices;
    imported.device_owners = baseline.device_owners;
    let mut current = store.profile_draft().unwrap().unwrap();
    current.devices.clear();
    current.device_owners.clear();
    store.save_profile_draft(&current).unwrap();
    let outcome = store
        .commit_profile_import(&identity, &attempt, &imported)
        .unwrap();
    assert_eq!(
        outcome.draft.devices,
        [] as [dyapp_identity::SignedRecord; 0]
    );
    assert_eq!(
        outcome.draft.device_owners,
        [] as [dyapp_identity::SignedRecord; 0]
    );
    assert_eq!(outcome.published, imported);
    assert!(outcome.pending);
}

#[test]
fn invalid_imported_photo_rolls_back_draft_and_search_projection() {
    let (identity, mut store, _, mut imported) = fixture();
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let before = store.profile_draft().unwrap().unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    imported.photos = before.photos.clone();
    imported.photos[0].thumbnail = vec![1];
    assert!(matches!(
        store.commit_profile_import(&identity, &attempt, &imported),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(before.clone()));
    assert!(store.due_search_profile(1000).unwrap().is_none());
    assert!(store
        .profile_photo(&before.photos[0].thumbnail)
        .unwrap()
        .is_some());
}

fn fixture() -> (Identity, Store, ProfilePublication, Profile) {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let baseline = Profile {
        version: 1,
        name: "old local".into(),
        ..Default::default()
    };
    store.save_profile_draft(&baseline).unwrap();
    let attempt = store.prepare_profile_publication(&identity).unwrap();
    let imported = Profile {
        version: 3,
        name: "remote".into(),
        ..baseline
    };
    (identity, store, attempt, imported)
}

#[test]
fn foreign_import_replaces_old_edits_and_queues_only_exact_remote_content() {
    let (identity, store, attempt, imported) = fixture();
    let outcome = store
        .commit_profile_import(&identity, &attempt, &imported)
        .unwrap();
    assert_eq!(outcome.published, imported);
    assert_eq!(outcome.draft, imported);
    assert!(!outcome.pending);
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(imported.sign(&identity))
    );
}

#[test]
fn foreign_import_keeps_current_visibility_and_pending_edits_out_of_search() {
    let (identity, mut store, attempt, imported) = fixture();
    store
        .update_profile_draft(|draft| {
            draft.name = "pending".into();
            Ok(())
        })
        .unwrap();
    store.set_profile_hidden(true).unwrap();
    let outcome = store
        .commit_profile_import(&identity, &attempt, &imported)
        .unwrap();
    assert_eq!(outcome.published, imported);
    assert_eq!(outcome.draft.name, "pending");
    assert_eq!(outcome.draft.version, 4);
    assert!(outcome.pending && outcome.hidden);
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(Profile::tombstone(4, 0, 0).sign(&identity))
    );
    assert!(store.profile_hidden().unwrap());
}

#[test]
fn foreign_import_refuses_newer_completion_and_wrong_owner_without_changes() {
    let (identity, store, attempt, imported) = fixture();
    let newer = Profile {
        version: 5,
        name: "newer".into(),
        ..imported.clone()
    };
    store.save_profile_draft(&newer).unwrap();
    assert!(matches!(
        store.commit_profile_import(&identity, &attempt, &imported),
        Err(Error::ProfileCompletionSuperseded)
    ));
    let stranger = Identity::from_secret(&[8; 32]);
    assert!(matches!(
        store.commit_profile_import(&stranger, &attempt, &imported),
        Err(Error::WrongIdentity)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(newer));
    assert!(store.due_search_profile(1000).unwrap().is_none());
}

#[test]
fn foreign_search_fork_rolls_back_the_earlier_draft_write() {
    let (identity, store, attempt, imported) = fixture();
    let before = store.profile_draft().unwrap();
    let fork = Profile {
        name: "other same version".into(),
        ..imported.clone()
    }
    .sign(&identity);
    store.hold_search_profile(&fork).unwrap();
    assert!(matches!(
        store.commit_profile_import(&identity, &attempt, &imported),
        Err(Error::SearchProfileConflict)
    ));
    assert_eq!(store.profile_draft().unwrap(), before);
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(fork));
}

#[test]
fn foreign_import_preserves_current_photo_bytes_and_stronger_proof() {
    let (identity, mut store, attempt, imported) = fixture();
    store
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let work = store.prepare_profile_work(&identity, 1, 1024).unwrap();
    assert!(store.commit_profile_work(&work).unwrap().solved);
    let current = store.profile_draft().unwrap().unwrap();
    let outcome = store
        .commit_profile_import(&identity, &attempt, &imported)
        .unwrap();
    assert_eq!(outcome.draft.photos, current.photos);
    assert_eq!(
        (outcome.draft.pow_bits, outcome.draft.pow_nonce),
        (current.pow_bits, current.pow_nonce)
    );
    for photo in &current.photos {
        for hash in photo.full.iter().chain([&photo.thumbnail]) {
            assert!(store.profile_photo(hash).unwrap().is_some());
        }
    }
    assert!(outcome.pending);
    assert_eq!(
        store.due_search_profile(1000).unwrap(),
        Some(imported.sign(&identity))
    );
}
