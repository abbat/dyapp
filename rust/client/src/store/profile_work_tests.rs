use super::*;
use crate::store::tests::temp_db_path;
use dyapp_profile::Profile;

fn owner() -> Identity {
    Identity::from_secret(&[7; 32])
}

fn solve(store: &mut Store, identity: &Identity, bits: u32) {
    let work = store.prepare_profile_work(identity, bits, 1024).unwrap();
    assert!(store.commit_profile_work(&work).unwrap().solved);
}

#[test]
fn prepared_work_preserves_fields_and_photos_and_does_not_acquire_the_writer() {
    let (path, identity) = (temp_db_path(), owner());
    let mut editor = Store::open(&path, &identity).unwrap();
    let mut worker = Store::open(&path, &identity).unwrap();
    worker.db.busy_timeout(std::time::Duration::ZERO).unwrap();
    editor.save_profile_draft(&Profile::default()).unwrap();
    let mut pending = None;
    editor
        .update_profile_draft(|draft| {
            // The independent writer is already held. Preparing work must still read/hash successfully.
            pending = Some(worker.prepare_profile_work(&identity, 257, 2)?);
            draft.about = "new field value".into();
            draft.version = 7;
            Ok(())
        })
        .unwrap();
    editor
        .add_profile_photo(&[0xFF, 0xD8, 0xFF, 1], &[0xFF, 0xD8, 0xFF, 2])
        .unwrap();
    let latest = editor.profile_draft().unwrap();
    let result = worker.commit_profile_work(&pending.unwrap()).unwrap();
    assert!(!result.solved);
    assert_eq!(result.next_nonce, 2);
    assert_eq!(editor.profile_draft().unwrap(), latest);
}

#[test]
fn resume_cursors_are_monotonic_per_target_and_do_not_invalidate_a_lower_proof() {
    let (path, identity) = (temp_db_path(), owner());
    let mut first = Store::open(&path, &identity).unwrap();
    let mut second = Store::open(&path, &identity).unwrap();
    solve(&mut first, &identity, 2);
    let proof = first.profile_draft().unwrap().unwrap();
    let slow = first.prepare_profile_work(&identity, 257, 2).unwrap();
    let fast = second.prepare_profile_work(&identity, 257, 5).unwrap();
    let advanced = second.commit_profile_work(&fast).unwrap();
    let late = first.commit_profile_work(&slow).unwrap();
    assert!(!late.solved);
    assert_eq!(late.next_nonce, advanced.next_nonce);
    assert_eq!(late.next_nonce, proof.pow_nonce.saturating_add(5));
    assert_eq!(first.profile_draft().unwrap(), Some(proof.clone()));
    assert!(
        dyapp_profile::pow_leading_bits(&identity.public_key(), proof.pow_nonce) >= proof.pow_bits
    );
    assert_eq!(
        second.profile_work_status(258).unwrap().next_nonce,
        proof.pow_nonce
    );
    let resumed = first.prepare_profile_work(&identity, 257, 0).unwrap();
    assert_eq!(
        first.commit_profile_work(&resumed).unwrap().next_nonce,
        late.next_nonce
    );
}

#[test]
fn a_late_weaker_solution_cannot_downgrade_a_stronger_solution() {
    let (path, identity) = (temp_db_path(), owner());
    let mut first = Store::open(&path, &identity).unwrap();
    let mut second = Store::open(&path, &identity).unwrap();
    let weak = first.prepare_profile_work(&identity, 1, 1024).unwrap();
    solve(&mut second, &identity, 2);
    let stronger = second.profile_draft().unwrap().unwrap();
    let result = first.commit_profile_work(&weak).unwrap();
    assert!(result.solved);
    assert_eq!(result.next_nonce, stronger.pow_nonce);
    assert_eq!(first.profile_draft().unwrap(), Some(stronger));
}

#[test]
fn a_late_pending_step_observes_a_concurrent_completed_proof() {
    let (path, identity) = (temp_db_path(), owner());
    let mut first = Store::open(&path, &identity).unwrap();
    let mut second = Store::open(&path, &identity).unwrap();
    let pending = first.prepare_profile_work(&identity, 2, 0).unwrap();
    solve(&mut second, &identity, 2);
    let completed = second.profile_draft().unwrap();
    assert!(first.commit_profile_work(&pending).unwrap().solved);
    assert_eq!(first.profile_draft().unwrap(), completed);
}

#[test]
fn work_cannot_be_prepared_or_committed_for_another_owner() {
    let identity = owner();
    let other = Identity::from_secret(&[9; 32]);
    let first = Store::open(&temp_db_path(), &identity).unwrap();
    let mut second = Store::open(&temp_db_path(), &other).unwrap();
    assert!(matches!(
        first.prepare_profile_work(&other, 257, 0),
        Err(Error::WrongIdentity)
    ));
    let pending = first.prepare_profile_work(&identity, 257, 0).unwrap();
    assert!(matches!(
        second.commit_profile_work(&pending),
        Err(Error::WrongIdentity)
    ));
    assert_eq!(second.profile_draft().unwrap(), None);
}

#[test]
fn corrupt_cursor_refuses_a_commit_without_replacing_the_draft() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 7,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    let pending = store.prepare_profile_work(&identity, 257, 0).unwrap();
    store
        .db
        .execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            params![key(257), vec![0u8; 7]],
        )
        .unwrap();
    assert!(matches!(
        store.profile_work_status(257),
        Err(Error::InvalidSetting)
    ));
    assert!(matches!(
        store.commit_profile_work(&pending),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(store.profile_draft().unwrap(), Some(before));
}

#[test]
fn draft_write_refusal_rolls_back_the_resume_cursor() {
    let identity = owner();
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = Profile {
        version: 7,
        ..Profile::default()
    };
    store.save_profile_draft(&before).unwrap();
    let pending = store.prepare_profile_work(&identity, 257, 2).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_proof_draft` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'profile_draft'
         BEGIN SELECT RAISE(ABORT, 'injected draft write refusal'); END;",
        )
        .unwrap();
    assert!(store.commit_profile_work(&pending).is_err());
    assert_eq!(store.profile_draft().unwrap(), Some(before));
    assert_eq!(store.profile_work_status(257).unwrap().next_nonce, 0);
}
