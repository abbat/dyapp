use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::{Domain, Identity};
use dyapp_profile::Profile;

fn assert_busy(result: &Result<(), Error>) {
    assert!(
        matches!(result, Err(Error::Sql(rusqlite::Error::SqliteFailure(error, _)))
        if error.code == rusqlite::ErrorCode::DatabaseBusy)
    );
}

fn record(identity: &Identity, version: u64) -> SignedRecord {
    Profile {
        version,
        ..Default::default()
    }
    .sign(identity)
}

#[test]
fn acknowledged_head_survives_restart_and_prevents_requeue() {
    let path = temp_db_path();
    let identity = Identity::generate();
    let store = Store::open(&path, &identity).unwrap();
    let newest = record(&identity, 8);
    store.hold_search_profile(&newest).unwrap();
    store.search_profile_sent(&newest, 1000).unwrap();
    drop(store);
    let store = Store::open(&path, &identity).unwrap();
    for retry in [record(&identity, 7), newest] {
        store.hold_search_profile(&retry).unwrap();
        assert!(store.due_search_profile(u64::MAX).unwrap().is_none());
    }
    assert_eq!(store.known_search_version().unwrap(), 8);
    let hidden = store
        .hide_profile_draft(&identity, &Profile::default())
        .unwrap();
    assert_eq!(hidden.version, 9);
}

#[test]
fn conflicting_or_unauthenticated_records_cannot_mutate_queue() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let original = record(&identity, 3);
    store.hold_search_profile(&original).unwrap();
    let conflict = Profile::tombstone(3, 0, 0).sign(&identity);
    assert!(matches!(
        store.hold_search_profile(&conflict),
        Err(Error::SearchProfileConflict)
    ));
    assert!(matches!(
        store.search_profile_sent(&conflict, 10),
        Err(Error::SearchProfileConflict)
    ));
    assert!(matches!(
        store.hold_search_profile(&record(&Identity::generate(), 4)),
        Err(Error::WrongIdentity)
    ));
    let mut tampered = record(&identity, 4);
    tampered.payload.push(0xff);
    assert!(matches!(
        store.hold_search_profile(&tampered),
        Err(Error::InvalidSetting)
    ));
    assert!(matches!(
        store.hold_search_profile(&identity.sign(Domain::Profile, vec![0xff])),
        Err(Error::InvalidSetting)
    ));
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(original));
}

#[test]
fn late_completions_preserve_newer_pending_and_monotonic_deadlines() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let old = record(&identity, 1);
    let new = record(&identity, 2);
    store.hold_search_profile(&new).unwrap();
    store.search_profile_sent(&old, 1000).unwrap();
    store.search_profile_sent(&old, 900).unwrap();
    assert_eq!(store.search_publish().unwrap().last_sent, 1000);
    store
        .search_profile_rate_limited(1000, Duration::from_secs(1000))
        .unwrap();
    store
        .search_profile_rate_limited(1000, Duration::from_secs(1))
        .unwrap();
    assert!(store.due_search_profile(1999).unwrap().is_none());
    assert_eq!(store.due_search_profile(2000).unwrap(), Some(new));
    store
        .search_profile_rate_limited(1, Duration::new(u64::MAX, 1))
        .unwrap();
    assert!(store.due_search_profile(u64::MAX - 1).unwrap().is_none());
    assert!(store.due_search_profile(u64::MAX).unwrap().is_some());
}

#[test]
fn legacy_pending_backfills_head_before_ack_clears_it() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let legacy = record(&identity, 5);
    store
        .set_search_publish(&State {
            pending: Some(legacy.clone()),
            ..Default::default()
        })
        .unwrap();
    assert!(store.search_head().unwrap().is_none());
    store.search_profile_sent(&legacy, 1000).unwrap();
    assert_eq!(store.search_head().unwrap().unwrap().version, 5);
    store.hold_search_profile(&record(&identity, 4)).unwrap();
    assert!(store.due_search_profile(u64::MAX).unwrap().is_none());
}

#[test]
fn independent_writers_are_refused_before_read_modify_write() {
    let path = temp_db_path();
    let identity = Identity::generate();
    let store = Store::open(&path, &identity).unwrap();
    let other = Store::open(&path, &identity).unwrap();
    other.db.busy_timeout(Duration::ZERO).unwrap();
    let first = record(&identity, 1);
    store.hold_search_profile(&first).unwrap();
    store
        .update_search(|_, _| {
            assert_busy(&other.hold_search_profile(&record(&identity, 2)));
            assert_busy(&other.search_profile_sent(&first, 1000));
            assert_busy(&other.search_profile_rate_limited(1000, Duration::from_secs(1)));
            Ok(())
        })
        .unwrap();
    assert_eq!(other.due_search_profile(1000).unwrap(), Some(first));
}

#[test]
fn failed_head_write_rolls_back_pending_and_ack_together() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    let first = record(&identity, 1);
    store.hold_search_profile(&first).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TEMP TRIGGER `reject_search_head` BEFORE INSERT ON `settings`
         WHEN NEW.`key` = 'search_profile_head' BEGIN SELECT RAISE(FAIL, 'head refused'); END;",
        )
        .unwrap();
    assert!(store.hold_search_profile(&record(&identity, 2)).is_err());
    assert!(store.search_profile_sent(&first, 1000).is_err());
    assert_eq!(store.due_search_profile(1000).unwrap(), Some(first));
    assert_eq!(store.known_search_version().unwrap(), 1);
    assert_eq!(store.search_publish().unwrap().last_sent, 0);
}

#[test]
fn malformed_head_fails_closed_and_saved_draft_is_a_version_floor() {
    let identity = Identity::generate();
    let store = Store::open(&temp_db_path(), &identity).unwrap();
    store
        .save_profile_draft(&Profile {
            version: 10,
            ..Default::default()
        })
        .unwrap();
    store.hold_search_profile(&record(&identity, 9)).unwrap();
    assert!(store.due_search_profile(1000).unwrap().is_none());
    for (format, value) in [(2, vec![0u8; 40]), (1, vec![0u8; 39])] {
        store.db.execute(
            "INSERT INTO `settings` (`key`, `format_version`, `value`) VALUES (?1, ?2, ?3)
             ON CONFLICT (`key`) DO UPDATE SET `format_version` = excluded.`format_version`, `value` = excluded.`value`",
            params![HEAD_KEY, format, value],
        ).unwrap();
        assert!(matches!(
            store.hold_search_profile(&record(&identity, 11)),
            Err(Error::InvalidSetting)
        ));
        assert!(matches!(
            store.due_search_profile(1000),
            Err(Error::InvalidSetting)
        ));
    }
}
