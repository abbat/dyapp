//! Prefix progress and pending lifetime share the database writer.
use super::*;
use crate::store::link_state::progress;

fn snapshot(store: &Store) -> LinkState {
    LinkState::decode(store.link_state().unwrap().as_slice()).unwrap()
}

#[test]
fn visibility_failure_rolls_back_prefix_clock_and_applied_phase() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    let before = snapshot(&store);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_import_visibility` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'profile_hidden'
         BEGIN SELECT RAISE(ABORT, 'visibility refused'); END;",
        )
        .unwrap();
    let error = store.import_link_state(&state).unwrap_err();
    assert!(error.to_string().contains("visibility refused"));
    assert_eq!(snapshot(&store), before);
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(false));
    assert_eq!(store.link_pending().unwrap(), Some(state));
}

#[test]
fn progress_failure_rolls_back_private_values_and_visibility() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    let before = snapshot(&store);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_applied_phase` BEFORE UPDATE ON `settings`
         WHEN NEW.`key` = 'link_state_import'
         BEGIN SELECT RAISE(ABORT, 'progress refused'); END;",
        )
        .unwrap();
    let error = store.import_link_state(&state).unwrap_err();
    assert!(error.to_string().contains("progress refused"));
    assert_eq!(snapshot(&store), before);
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(false));
}

#[test]
fn pending_creation_failure_rolls_back_linked_marker_and_initial_phase() {
    let identity = Identity::from_secret(&[7; 32]);
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let state = source(&identity);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_pending` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'link_pending'
         BEGIN SELECT RAISE(ABORT, 'pending refused'); END;",
        )
        .unwrap();
    assert!(store.set_link_pending(&state).is_err());
    assert_eq!(store.link_pending().unwrap(), None);
    assert!(!store.is_linked_device().unwrap());
    assert_eq!(progress::read(&store.db, &state).unwrap(), None);
}

#[test]
fn clear_failure_is_atomic_and_a_new_same_bytes_link_does_not_reuse_applied_progress() {
    let identity = Identity::from_secret(&[7; 32]);
    let path = temp_db_path();
    let state = source(&identity);
    let mut store = Store::open(&path, &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    store.import_link_state(&state).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_clear_progress` BEFORE DELETE ON `settings`
         WHEN OLD.`key` = 'link_state_import'
         BEGIN SELECT RAISE(ABORT, 'clear refused'); END;",
        )
        .unwrap();
    assert!(store.clear_link_pending().is_err());
    assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(true));
    drop(store);
    let mut store = Store::open(&path, &identity).unwrap();
    store.clear_link_pending().unwrap();
    assert_eq!(store.link_pending().unwrap(), None);
    assert_eq!(progress::read(&store.db, &state).unwrap(), None);
    assert!(store.is_linked_device().unwrap());
    store.set_profile_hidden(false).unwrap();
    store.set_link_pending(&state).unwrap();
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(false));
    store.import_link_state(&state).unwrap();
    assert!(store.profile_hidden().unwrap());
}

#[test]
fn a_replaced_pending_snapshot_requires_its_own_exact_initial_phase() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    store.import_link_state(&state).unwrap();
    private_choices(&store, false, 2_000);
    let before = snapshot(&store);
    // Unknown wire fields keep the decoded values but still name a different exact snapshot.
    let mut different = state.clone();
    different.extend([0xF8, 0x07, 0x01]);
    assert!(matches!(
        store.import_link_state(&different),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(snapshot(&store), before);
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(true));
    store.set_link_pending(&different).unwrap();
    assert_eq!(progress::read(&store.db, &different).unwrap(), Some(false));
    store.import_link_state(&different).unwrap();
    assert!(store.blocked("carol").unwrap());
    assert_eq!(store.link_pending().unwrap(), Some(different));
}

#[test]
fn an_old_completion_cannot_clear_a_replacement_or_unapplied_pending_link() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    assert!(matches!(
        store.clear_completed_link(&state),
        Err(Error::LinkStateChanged)
    ));
    store.import_link_state(&state).unwrap();
    let different = LinkState {
        hidden: false,
        ..LinkState::decode(state.as_slice()).unwrap()
    }
    .encode_to_vec();
    store.set_link_pending(&different).unwrap();
    assert!(matches!(
        store.clear_completed_link(&state),
        Err(Error::LinkStateChanged)
    ));
    assert_eq!(store.link_pending().unwrap(), Some(different.clone()));
    assert_eq!(progress::read(&store.db, &different).unwrap(), Some(false));
    store.import_link_state(&different).unwrap();
    store.clear_completed_link(&different).unwrap();
    assert_eq!(store.link_pending().unwrap(), None);
}

#[test]
fn malformed_progress_is_refused_before_any_prefix_write() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    store.set_link_pending(&state).unwrap();
    let before = snapshot(&store);
    let mut wrong_digest = vec![0_u8; 33];
    wrong_digest[1] = 1;
    for (format, value) in [
        (2, vec![0; 33]),
        (1, vec![]),
        (1, vec![0; 32]),
        (1, vec![2; 33]),
        (1, wrong_digest),
    ] {
        store
            .db
            .execute(
                "UPDATE `settings` SET `format_version` = ?1, `value` = ?2 WHERE `key` = ?3",
                rusqlite::params![format, value, progress::KEY],
            )
            .unwrap();
        assert!(matches!(
            store.import_link_state(&state),
            Err(Error::InvalidSetting)
        ));
        assert_eq!(snapshot(&store), before);
        assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
    }
}

fn legacy_pending(store: &Store, state: &[u8]) {
    store
        .db
        .execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            rusqlite::params![PENDING, state],
        )
        .unwrap();
}

#[test]
fn identical_legacy_snapshot_records_progress_without_replaying_private_writes() {
    let identity = Identity::from_secret(&[7; 32]);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    private_choices(&store, false, 2_000);
    let state = store.link_state().unwrap();
    legacy_pending(&store, &state);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_peer_replay` BEFORE INSERT ON `peers`
         WHEN NOT EXISTS (SELECT 1 FROM `settings` WHERE `key` = 'link_state_import')
         BEGIN SELECT RAISE(ABORT, 'private replay refused'); END;",
        )
        .unwrap();
    let writes = store.db.total_changes();
    store.import_link_state(&state).unwrap();
    // Only the applied-progress row is written, even when every private value is unchanged.
    assert_eq!(store.db.total_changes() - writes, 1);
    assert_eq!(store.link_state().unwrap(), state);
    assert_eq!(progress::read(&store.db, &state).unwrap(), Some(true));
    assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
    private_choices(&store, true, 3_000);
    let changed = store.link_state().unwrap();
    store.import_link_state(&state).unwrap();
    assert_eq!(store.link_state().unwrap(), changed);
}

#[test]
fn unknown_wire_fields_do_not_prove_identical_legacy_state() {
    let identity = Identity::from_secret(&[7; 32]);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    let before = store.link_state().unwrap();
    let mut state = before.clone();
    state.extend([0xF8, 0x07, 0x01]);
    legacy_pending(&store, &state);
    assert!(matches!(
        store.import_link_state(&state),
        Err(Error::LinkStateReviewRequired)
    ));
    assert_eq!(store.link_state().unwrap(), before);
    assert_eq!(progress::read(&store.db, &state).unwrap(), None);
}

#[test]
fn identical_legacy_progress_failure_preserves_the_unresolved_operation() {
    let identity = Identity::from_secret(&[7; 32]);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    private_choices(&store, false, 2_000);
    let state = store.link_state().unwrap();
    legacy_pending(&store, &state);
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `refuse_legacy_progress` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'link_state_import'
         BEGIN SELECT RAISE(ABORT, 'progress refused'); END;",
        )
        .unwrap();
    assert!(store
        .import_link_state(&state)
        .unwrap_err()
        .to_string()
        .contains("progress refused"));
    assert_eq!(store.link_state().unwrap(), state);
    assert_eq!(store.link_pending().unwrap(), Some(state.clone()));
    assert_eq!(progress::read(&store.db, &state).unwrap(), None);
}

#[test]
fn legacy_pending_without_progress_keeps_both_private_snapshots_for_explicit_recovery() {
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let mut store = Store::open(&temp_db_path(), &identity).unwrap();
    private_choices(&store, false, 2_000);
    let before = snapshot(&store);
    store
        .db
        .execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, 1, ?2)",
            rusqlite::params![PENDING, state],
        )
        .unwrap();
    store.set_link_pending(&state).unwrap();
    assert_eq!(progress::read(&store.db, &state).unwrap(), None);
    assert!(matches!(
        store.import_link_state(&state),
        Err(Error::LinkStateReviewRequired)
    ));
    assert_eq!(snapshot(&store), before);
    assert_eq!(store.link_pending().unwrap(), Some(state));
}
