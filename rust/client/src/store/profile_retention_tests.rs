use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Identity;

fn bytes(seed: u8) -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, seed]
}

#[test]
fn cleanup_keeps_remote_and_current_draft_references_but_prunes_unused_blobs() {
    let (path, owner) = (temp_db_path(), Identity::generate());
    let store = Store::open(&path, &owner).unwrap();
    let remote = store.save_profile_blob(&bytes(1)).unwrap();
    let unused = store.save_profile_blob(&bytes(2)).unwrap();
    let mut editor = Store::open(&path, &owner).unwrap();
    let current = editor.add_profile_photo(&bytes(3), &bytes(4)).unwrap();
    store
        .retain_profile_photos(std::slice::from_ref(&remote))
        .unwrap();
    assert!(store.profile_photo(&remote).unwrap().is_some());
    assert!(store.profile_photo(&unused).unwrap().is_none());
    for hash in current
        .photos
        .iter()
        .flat_map(|photo| photo.full.iter().chain(std::iter::once(&photo.thumbnail)))
    {
        assert!(store.profile_photo(hash).unwrap().is_some());
    }
    assert_eq!(editor.profile_draft().unwrap(), Some(current));
    editor.remove_profile_photo(0).unwrap();
    store.retain_profile_photos(&[]).unwrap();
    assert!(!store.has_profile_photos().unwrap());
}

#[test]
fn even_noop_cleanup_admits_a_writer_before_observing_current_references() {
    let (path, owner) = (temp_db_path(), Identity::generate());
    let mut editor = Store::open(&path, &owner).unwrap();
    let pruner = Store::open(&path, &owner).unwrap();
    pruner.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let photo = editor
        .add_profile_photo(&bytes(1), &bytes(2))
        .unwrap()
        .photos
        .remove(0);
    let published = vec![photo.full[0].clone(), photo.thumbnail.clone()];
    editor
        .update_profile_draft(|draft| {
            assert!(matches!(
                pruner.retain_profile_photos(&published),
                Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _)))
                    if error.code == rusqlite::ErrorCode::DatabaseBusy
            ));
            draft.photos.clear();
            Ok(())
        })
        .unwrap();
    pruner.retain_profile_photos(&published).unwrap();
    for hash in published {
        assert!(pruner.profile_photo(&hash).unwrap().is_some());
    }
}

#[test]
fn invalid_published_or_draft_hashes_refuse_cleanup_without_deleting_bytes() {
    let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
    let hash = store.save_profile_blob(&bytes(1)).unwrap();
    assert!(matches!(
        store.retain_profile_photos(&[vec![0; 31]]),
        Err(Error::InvalidSetting)
    ));
    assert!(store.profile_photo(&hash).unwrap().is_some());
    let corrupt = Profile {
        photos: vec![dyapp_profile::Photo {
            full: vec![vec![0; 31]],
            thumbnail: vec![0; 32],
        }],
        ..Profile::default()
    };
    store.save_profile_draft(&corrupt).unwrap();
    assert!(matches!(
        store.retain_profile_photos(&[]),
        Err(Error::InvalidSetting)
    ));
    assert!(store.profile_photo(&hash).unwrap().is_some());
}

#[test]
fn a_failure_on_the_second_delete_rolls_back_the_entire_cleanup() {
    let (path, owner) = (temp_db_path(), Identity::generate());
    let store = Store::open(&path, &owner).unwrap();
    let mut other = Store::open(&path, &owner).unwrap();
    other.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let hashes = [
        store.save_profile_blob(&bytes(1)).unwrap(),
        store.save_profile_blob(&bytes(2)).unwrap(),
    ];
    // Count actual delete completions; no assumption about query/hash enumeration order.
    store
        .db
        .execute_batch(
            "CREATE TEMP TABLE `prune_steps` (`n` INTEGER NOT NULL);
         INSERT INTO `prune_steps` (`n`) VALUES (0);
         CREATE TEMP TRIGGER `count_pruned` AFTER DELETE ON `settings`
         BEGIN UPDATE `prune_steps` SET `n` = `n` + 1; END;
         CREATE TEMP TRIGGER `refuse_second_prune` BEFORE DELETE ON `settings`
         WHEN (SELECT `n` FROM `prune_steps`) = 1
         BEGIN SELECT RAISE(ABORT, 'second prune refused'); END;",
        )
        .unwrap();
    let error = store.retain_profile_photos(&[]).unwrap_err();
    assert!(error.to_string().contains("second prune refused"));
    for hash in hashes {
        assert!(other.profile_photo(&hash).unwrap().is_some());
    }
    let steps: i64 = store
        .db
        .query_row("SELECT `n` FROM `prune_steps`", [], |row| row.get(0))
        .unwrap();
    assert_eq!(steps, 0);
    other
        .update_profile_draft(|draft| {
            draft.version = 7;
            Ok(())
        })
        .unwrap();
}

#[test]
fn lookalike_settings_are_outside_the_photo_namespace() {
    let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
    store.save_profile_blob(&bytes(1)).unwrap();
    for prefix in ["profileXphoto:", "PROFILE_PHOTO:"] {
        let name = format!("{prefix}{}", "0".repeat(64));
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
                rusqlite::params![name, bytes(2)],
            )
            .unwrap();
    }
    store.retain_profile_photos(&[]).unwrap();
    assert!(!store.has_profile_photos().unwrap());
    for prefix in ["profileXphoto:", "PROFILE_PHOTO:"] {
        let name = format!("{prefix}{}", "0".repeat(64));
        let value: Vec<u8> = store
            .db
            .query_row(
                "SELECT `value` FROM `settings` WHERE `key` = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, bytes(2));
    }
}

#[test]
fn unknown_blob_keys_or_formats_refuse_cleanup_before_any_deletion() {
    for (suffix, version) in [("0".repeat(64), 2), ("unsupported".to_owned(), 1)] {
        let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
        let known = store.save_profile_blob(&bytes(1)).unwrap();
        store
            .db
            .execute(
                "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("profile_photo:{suffix}"), version, bytes(2)],
            )
            .unwrap();
        assert!(matches!(
            store.retain_profile_photos(&[]),
            Err(Error::InvalidSetting)
        ));
        assert!(store.profile_photo(&known).unwrap().is_some());
    }
}

#[test]
fn internal_retention_obeys_the_callers_transaction_boundary() {
    let store = Store::open(&temp_db_path(), &Identity::generate()).unwrap();
    let hash = store.save_profile_blob(&bytes(1)).unwrap();
    let transaction =
        Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
    retain(&transaction, &[], &Profile::default()).unwrap();
    assert!(store.profile_photo(&hash).unwrap().is_none());
    transaction.rollback().unwrap();
    assert!(store.profile_photo(&hash).unwrap().is_some());
}
