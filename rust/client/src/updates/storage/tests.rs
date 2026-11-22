use super::*;
use crate::updates::{coordinator::Checkpoint, transport::Cache, CheckState, Interval};
use dyapp_test_support::test_dir;

fn snapshot(mut snapshot: Snapshot) -> Snapshot {
    let body = br#"{"schema":1,"channel":"stable","targets":{"android":{"version":"2.0.0","tag":"v2.0.0","method":"package"}}}"#;
    snapshot
        .admit(Cache::restore(body.to_vec(), Some("\"disk\"")).unwrap())
        .unwrap();
    snapshot.set_checkpoint(Checkpoint {
        interval: Interval::TwelveHours,
        state: CheckState {
            last_attempt: Some(500),
            last_success: Some(100),
            consecutive_failures: 1,
        },
        in_flight: true,
    });
    snapshot
}

#[test]
fn initialization_generates_jitter_once_and_preserves_all_seed_boundaries() {
    for jitter in [0, 1, u64::MAX] {
        let mut database = Connection::open_in_memory().unwrap();
        initialize(&mut database, || Ok(Snapshot::fresh(jitter))).unwrap();
        initialize(&mut database, || {
            panic!("existing state must not regenerate jitter")
        })
        .unwrap();
        let saved = read_snapshot(&database).unwrap();
        assert_eq!(saved.jitter(), jitter);
        assert_eq!(
            Snapshot::decode(&saved.encode().unwrap()).unwrap().jitter(),
            jitter
        );
    }
    let mut database = Connection::open_in_memory().unwrap();
    assert!(matches!(
        initialize(&mut database, || Err(Error::Entropy)),
        Err(Error::Entropy)
    ));
    initialize(&mut database, || Ok(Snapshot::fresh(7))).unwrap();
    assert_eq!(read_snapshot(&database).unwrap().jitter(), 7);
}

#[test]
fn committed_snapshot_reopens_exactly_under_one_installation_lease() {
    let dir = test_dir().unwrap();
    let mut state = LockedState::try_open(&dir).unwrap().unwrap();
    assert_eq!(state.load().unwrap().checkpoint(), Checkpoint::default());
    let jitter = state.load().unwrap().jitter();
    assert!(matches!(
        state.save(&Snapshot::fresh(jitter ^ 1)),
        Err(Error::Incompatible)
    ));
    assert_eq!(state.load().unwrap().jitter(), jitter);
    let expected = snapshot(state.load().unwrap()).encode().unwrap();
    state.save(&snapshot(state.load().unwrap())).unwrap();
    assert!(LockedState::try_open(&dir).unwrap().is_none());
    drop(state);
    let reopened = LockedState::try_open(&dir).unwrap().unwrap();
    assert_eq!(reopened.load().unwrap().encode().unwrap(), expected);
}

#[test]
fn profile_selection_lock_does_not_admit_or_interrupt_an_update() {
    let dir = test_dir().unwrap();
    let profile = regular_file(&dir.join("profiles.lock")).unwrap();
    profile.lock().unwrap();
    let mut state = LockedState::try_open(&dir).unwrap().unwrap();
    state.save(&snapshot(state.load().unwrap())).unwrap();
    assert!(LockedState::try_open(&dir).unwrap().is_none());
    let other_profile = regular_file(&dir.join("profiles.lock")).unwrap();
    assert!(matches!(
        other_profile.try_lock(),
        Err(TryLockError::WouldBlock)
    ));
    drop(state);
    assert!(LockedState::try_open(&dir).unwrap().is_some());
}

#[test]
fn failed_validation_or_transaction_preserves_the_previous_record() {
    let dir = test_dir().unwrap();
    let mut state = LockedState::try_open(&dir).unwrap().unwrap();
    state.save(&snapshot(state.load().unwrap())).unwrap();
    let before = state.load().unwrap().encode().unwrap();
    let mut invalid = Snapshot::default();
    invalid.set_checkpoint(Checkpoint {
        in_flight: true,
        ..Checkpoint::default()
    });
    assert!(state.save(&invalid).is_err());
    assert_eq!(state.load().unwrap().encode().unwrap(), before);
    assert!(matches!(
        state.save(&Snapshot::default()),
        Err(Error::Snapshot(snapshot::Error::Rollback))
    ));
    state.connection.execute_batch("CREATE TRIGGER `fail_save` BEFORE UPDATE ON `state` BEGIN SELECT RAISE(ABORT, 'controlled disk failure'); END;").unwrap();
    let mut next = state.load().unwrap();
    let mut checkpoint = next.checkpoint();
    checkpoint.interval = Interval::Week;
    next.set_checkpoint(checkpoint);
    assert!(matches!(state.save(&next), Err(Error::Sql(_))));
    assert_eq!(state.load().unwrap().encode().unwrap(), before);
}

#[test]
fn unknown_schema_and_missing_or_corrupt_payloads_are_not_repaired() {
    let oversized = vec![b' '; snapshot::MAX_BYTES + 1];
    for bytes in [b"{}".as_slice(), b"not a checkpoint", oversized.as_slice()] {
        let dir = test_dir().unwrap();
        let state = LockedState::try_open(&dir).unwrap().unwrap();
        state
            .connection
            .execute("UPDATE `state` SET `payload` = ?1", params![bytes])
            .unwrap();
        drop(state);
        assert!(LockedState::try_open(&dir).is_err());
        let database = Connection::open(dir.join("updates.sqlite")).unwrap();
        let retained: Vec<u8> = database
            .query_row("SELECT `payload` FROM `state`", [], |row| row.get(0))
            .unwrap();
        assert_eq!(retained, bytes);
    }
    let dir = test_dir().unwrap();
    let state = LockedState::try_open(&dir).unwrap().unwrap();
    state
        .connection
        .pragma_update(None, "user_version", 99)
        .unwrap();
    drop(state);
    assert!(LockedState::try_open(&dir).is_err());
    let database = Connection::open(dir.join("updates.sqlite")).unwrap();
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

#[test]
fn absent_singleton_foreign_database_and_non_regular_paths_fail_closed() {
    let dir = test_dir().unwrap();
    let state = LockedState::try_open(&dir).unwrap().unwrap();
    state
        .connection
        .execute("DELETE FROM `state` WHERE `id` = 1", [])
        .unwrap();
    drop(state);
    assert!(LockedState::try_open(&dir).is_err());
    let foreign = test_dir().unwrap();
    let database = Connection::open(foreign.join("updates.sqlite")).unwrap();
    database
        .execute_batch("CREATE TABLE `foreign` (`retained` INTEGER);")
        .unwrap();
    drop(database);
    assert!(LockedState::try_open(&foreign).is_err());
    assert!(LockedState::try_open(Path::new("relative-installation")).is_err());
    let bad = test_dir().unwrap();
    fs::create_dir(bad.join("updates.lock")).unwrap();
    assert!(LockedState::try_open(&bad).is_err());
}
