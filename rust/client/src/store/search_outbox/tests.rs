use super::*;
use crate::store::tests::temp_db_path;
use dyapp_identity::Identity;
use dyapp_profile::Profile;
use std::time::Duration;

fn at(unix_seconds: u64) -> SearchOutboxSchedule {
    SearchOutboxSchedule::At { unix_seconds }
}

#[test]
fn ordinary_queue_exposes_the_exact_window_and_backoff_across_reopen() {
    let owner = Identity::generate();
    let path = temp_db_path();
    let store = Store::open(&path, &owner).unwrap();
    assert_eq!(
        store.search_outbox_schedule().unwrap(),
        SearchOutboxSchedule::Idle
    );
    let first = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    store.search_profile_sent(&first, 1000).unwrap();
    let next = Profile {
        version: 2,
        ..Default::default()
    }
    .sign(&owner);
    store.hold_search_profile(&next).unwrap();
    assert_eq!(store.search_outbox_schedule().unwrap(), at(1600));
    store
        .search_profile_rate_limited(1000, Duration::from_secs(120))
        .unwrap();
    assert_eq!(store.search_outbox_schedule().unwrap(), at(1600));
    store
        .search_profile_rate_limited(1000, Duration::from_mins(20))
        .unwrap();
    drop(store);
    let store = Store::open_existing(&path, &owner).unwrap();
    let before = store.search_projection().unwrap();
    assert_eq!(store.search_outbox_schedule().unwrap(), at(2200));
    assert_eq!(store.due_search_publication(2199).unwrap(), None);
    assert_eq!(store.due_search_publication(2200).unwrap(), Some(next));
    assert!(store.search_projection().unwrap() == before);
}

#[test]
fn legacy_queue_requires_review_even_before_a_deferred_deadline_and_never_migrates() {
    let owner = Identity::generate();
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let legacy = Profile::tombstone(2, 0, 0).sign(&owner);
    store.hold_search_profile(&legacy).unwrap();
    store
        .search_profile_rate_limited(0, Duration::from_secs(u64::MAX))
        .unwrap();
    let before = store.search_projection().unwrap();
    assert_eq!(
        store.search_outbox_schedule().unwrap(),
        SearchOutboxSchedule::ReviewRequired
    );
    assert!(matches!(
        store.due_search_publication(0),
        Err(Error::SearchWithdrawalReviewRequired)
    ));
    assert_eq!(store.due_search_profile(u64::MAX).unwrap(), Some(legacy));
    assert!(store.search_projection().unwrap() == before);
}

#[test]
fn authorized_withdrawal_bypasses_only_the_window_and_ack_becomes_idle() {
    let owner = Identity::generate();
    let path = temp_db_path();
    let store = Store::open(&path, &owner).unwrap();
    let first = Profile {
        version: 1,
        ..Default::default()
    }
    .sign(&owner);
    store.search_profile_sent(&first, 1000).unwrap();
    let hidden = Profile::search_tombstone(2, 0, 0).sign(&owner);
    store.hold_search_profile(&hidden).unwrap();
    assert_eq!(store.search_outbox_schedule().unwrap(), at(0));
    store
        .search_profile_rate_limited(1000, Duration::from_millis(120_001))
        .unwrap();
    drop(store);
    let store = Store::open_existing(&path, &owner).unwrap();
    assert_eq!(store.search_outbox_schedule().unwrap(), at(1121));
    assert_eq!(store.due_search_publication(1120).unwrap(), None);
    assert_eq!(
        store.due_search_publication(1121).unwrap(),
        Some(hidden.clone())
    );
    store.search_profile_sent(&hidden, 1121).unwrap();
    assert_eq!(
        store.search_outbox_schedule().unwrap(),
        SearchOutboxSchedule::Idle
    );
    assert_eq!(store.due_search_publication(u64::MAX).unwrap(), None);
}
