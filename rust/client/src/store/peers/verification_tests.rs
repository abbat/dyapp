//! Storage boundaries for exact contact authority, independent of platform adapters.
use super::super::{tests::temp_db_path, Store};
use super::verification::{floor, Floor};
use dyapp_identity::Identity;
use dyapp_profile::{Profile, VerifiedProfile};
use rusqlite::{params, Transaction, TransactionBehavior};

fn profile(version: u64) -> VerifiedProfile {
    let owner = Identity::from_secret(&[2; 32]);
    dyapp_profile::verify(
        &Profile {
            version,
            ..Default::default()
        }
        .sign(&owner),
    )
    .unwrap()
}

#[test]
fn exact_floor_validation_refuses_corrupt_and_future_encodings() {
    let maximum = u64::MAX.to_be_bytes();
    assert!(matches!(floor(None, None).unwrap(), Floor::Absent));
    assert!(matches!(floor(Some(3), None).unwrap(), Floor::Exact(3)));
    assert!(matches!(
        floor(Some(i64::MAX), None).unwrap(),
        Floor::Ambiguous
    ));
    assert!(matches!(
        floor(Some(i64::MAX), Some(&maximum)).unwrap(),
        Floor::Exact(u64::MAX)
    ));
    for (legacy, bytes) in [
        (None, Some(maximum.as_slice())),
        (Some(-1), None),
        (Some(0), None),
        (Some(i64::MAX), Some(&maximum[..7])),
        (Some(3), Some(maximum.as_slice())),
        (Some(0), Some(&[0; 8])),
    ] {
        assert!(floor(legacy, bytes).is_err());
    }
}

#[test]
fn enclosing_transaction_controls_the_exact_floor_and_verification_mark() {
    let owner = Identity::from_secret(&[1; 32]);
    let path = temp_db_path();
    let store = Store::open(&path, &owner).unwrap();
    store.set_verified(&profile(3)).unwrap();
    let transaction =
        Transaction::new_unchecked(&store.db, TransactionBehavior::Immediate).unwrap();
    store.set_verified(&profile(u64::MAX)).unwrap();
    assert!(store.verified(&profile(u64::MAX)).unwrap());
    transaction.rollback().unwrap();
    let reopened = Store::open_existing(&path, &owner).unwrap();
    // The maximum was not committed by either nested method.
    reopened.set_verified(&profile(4)).unwrap();
    assert!(reopened.verified(&profile(4)).unwrap());
}

#[test]
fn recoverable_legacy_floor_is_upgraded_only_after_successful_observation() {
    let owner = Identity::from_secret(&[1; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let held = profile(3);
    store.set_verified(&held).unwrap();
    store
        .db
        .execute(
            "UPDATE `peers` SET `seen_version_exact` = NULL WHERE `peer_id` = ?1",
            [&held.peer_id],
        )
        .unwrap();
    assert!(store.set_verified(&profile(2)).is_err());
    assert!(store.verified(&profile(4)).unwrap());
    let exact: Vec<u8> = store
        .db
        .query_row(
            "SELECT `seen_version_exact` FROM `peers` WHERE `peer_id` = ?1",
            [&held.peer_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(exact, 4_u64.to_be_bytes());
}

#[test]
fn snapshots_cannot_discard_exact_authority_or_equal_version_device_memory() {
    let owner = Identity::from_secret(&[1; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let held = profile(u64::MAX);
    store.set_verified(&held).unwrap();
    let exact = u64::MAX.to_be_bytes();
    assert!(super::verification::admit_snapshot(
        &store.db,
        &held.peer_id,
        Some(i64::MAX),
        None,
        Some(&[]),
    )
    .is_err());
    assert!(super::verification::admit_snapshot(
        &store.db,
        &held.peer_id,
        Some(i64::MAX),
        Some(&exact),
        Some(&[3; 32]),
    )
    .is_err());
    assert!(super::verification::admit_snapshot(
        &store.db,
        &held.peer_id,
        Some(i64::MAX),
        Some(&exact),
        Some(&[]),
    )
    .is_ok());
    assert!(store.verified(&held).unwrap());
}

#[test]
fn failed_device_observation_rolls_back_exact_floor_and_notice_together() {
    let owner = Identity::from_secret(&[1; 32]);
    let store = Store::open(&temp_db_path(), &owner).unwrap();
    let held = profile(3);
    store.set_verified(&held).unwrap();
    store
        .db
        .execute_batch(
            "CREATE TRIGGER refuse_observation BEFORE UPDATE ON `peers`
         BEGIN SELECT RAISE(ABORT, 'controlled observation failure'); END;",
        )
        .unwrap();
    assert!(store.verified(&profile(u64::MAX)).is_err());
    let row: (Vec<u8>, bool, bool) = store.db.query_row(
        "SELECT `seen_version_exact`, `verified`, `new_device` FROM `peers` WHERE `peer_id` = ?1",
        params![held.peer_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).unwrap();
    assert_eq!(row, (3_u64.to_be_bytes().to_vec(), true, false));
}
