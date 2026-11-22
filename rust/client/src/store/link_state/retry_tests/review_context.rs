//! Revalidation shares the confirmation writer without creating a transaction or authority.
use super::*;
use rusqlite::{Transaction, TransactionBehavior};

#[test]
fn review_revalidation_inside_writer_is_read_only_and_observes_rollback() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let store = Store::open(&test_dir.path().join("client-1.db"), &identity).unwrap();
    legacy(&store, &state);
    let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
    assert!(matches!(
        review.is_current_in_snapshot(&store, &identity),
        Err(Error::InvalidSetting)
    ));
    let writer = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
    let writes = store.db.total_changes();
    assert!(review.is_current_in_snapshot(&store, &identity).unwrap());
    assert_eq!(store.db.total_changes(), writes);
    let other = Identity::from_secret(&[9; 32]);
    assert!(matches!(
        review.is_current_in_snapshot(&store, &other),
        Err(Error::WrongIdentity)
    ));
    store.set_profile_hidden(true).unwrap();
    assert!(!review.is_current_in_snapshot(&store, &identity).unwrap());
    drop(writer);
    assert!(review.is_current(&store, &identity).unwrap());
}

#[test]
fn review_writer_revalidation_detects_progress_and_pending_replacement() {
    let test_dir = dyapp_test_support::test_dir().unwrap();
    let identity = Identity::from_secret(&[7; 32]);
    let state = source(&identity);
    let store = Store::open(&test_dir.path().join("client-1.db"), &identity).unwrap();
    legacy(&store, &state);
    let review = store.prepare_link_state_review(&identity).unwrap().unwrap();
    let writer = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
    crate::store::link_state::progress::save(&store.db, &state, true).unwrap();
    assert!(!review.is_current_in_snapshot(&store, &identity).unwrap());
    drop(writer);
    let writer = Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
    store
        .db
        .execute(
            "UPDATE `settings` SET `value` = ?1 WHERE `key` = ?2",
            rusqlite::params![LinkState::default().encode_to_vec(), PENDING],
        )
        .unwrap();
    assert!(!review.is_current_in_snapshot(&store, &identity).unwrap());
    drop(writer);
    assert!(review.is_current(&store, &identity).unwrap());
}
