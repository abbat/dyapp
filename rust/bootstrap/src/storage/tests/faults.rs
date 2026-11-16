//! Deterministic SQLite write failures, process crash and reopen checks.

use super::*;
use std::path::PathBuf;

const MAILBOX: [u8; 32] = [5; 32];
const CRASH_DIR: &str = "DYAPP_TEST_ACK_CRASH_DIR";
const CRASH_TEST: &str = "storage::tests::faults::mailbox_ack_crash_rolls_back_on_reopen";

fn fault_store() -> (PathBuf, BootstrapStore) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/storage-faults")
        .join(uuid::Uuid::new_v4().to_string());
    let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
    (dir, store)
}

fn put(store: &BootstrapStore, id: u8, bytes: usize) {
    let record = SignedRecord {
        payload: vec![id; bytes],
        ..SignedRecord::default()
    };
    assert!(store
        .put_plain(&MAILBOX, &[id], &record, false, i64::MAX, u64::MAX)
        .unwrap());
}

fn fail_ack_delete(db: &Connection, target: u8) {
    let id = match target {
        1 => "x'01'",
        2 => "x'02'",
        _ => unreachable!(),
    };
    db.execute_batch(&format!(
        "CREATE TRIGGER `fail_ack` BEFORE DELETE ON `envelopes`
         WHEN OLD.`id` = {id}
         BEGIN SELECT RAISE(ABORT, 'injected storage fault'); END;"
    ))
    .unwrap();
}

fn seed_ack(store: &BootstrapStore) {
    put(store, 1, 1);
    put(store, 2, 1);
}

fn ack_ids(store: &BootstrapStore) -> Vec<Vec<u8>> {
    store.envelope_ids(&MAILBOX, 10).unwrap()
}

#[test]
fn profile_replica_apply_failure_keeps_reads_and_survives_reopen() {
    let (dir, store) = fault_store();
    let owner = Identity::from_secret(&[7; 32]);
    let record = Profile {
        version: 1,
        ..Profile::default()
    }
    .sign(&owner);
    {
        let db = lock(&store.profiles).unwrap();
        db.execute_batch(
            "CREATE TRIGGER `fail_profile` BEFORE INSERT ON `profiles`
             BEGIN SELECT RAISE(ABORT, 'injected storage fault'); END;",
        )
        .unwrap();
    }
    assert!(store.put_profile_seen(&record, 100).is_err());
    assert!(store.get_profile(&owner.peer_id()).unwrap().is_none());
    drop(store);
    let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
    assert!(store.get_profile(&owner.peer_id()).unwrap().is_none());
}

#[test]
fn every_mailbox_ack_delete_is_atomic_across_reopen() {
    for target in 1..=2 {
        let (dir, store) = fault_store();
        seed_ack(&store);
        {
            let db = lock(&store.messages).unwrap();
            fail_ack_delete(&db, target);
        }
        assert!(store.ack_envelopes(&MAILBOX, &[vec![1], vec![2]]).is_err());
        assert_eq!(ack_ids(&store), [vec![1], vec![2]], "fault {target}");
        drop(store);
        let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
        assert_eq!(ack_ids(&store), [vec![1], vec![2]], "fault {target}");
    }
}

#[test]
fn full_and_read_only_stores_report_write_errors_and_serve_reads() {
    let (_, store) = fault_store();
    seed_ack(&store);
    let pages = {
        let db = lock(&store.messages).unwrap();
        let pages = db
            .query_row("PRAGMA `page_count`", [], |row| row.get::<_, i64>(0))
            .unwrap();
        db.execute_batch(&format!("PRAGMA `max_page_count` = {pages}"))
            .unwrap();
        pages
    };
    assert!(pages > 0);
    let record = SignedRecord {
        payload: vec![3; 1024 * 1024],
        ..SignedRecord::default()
    };
    assert!(store
        .put_plain(&MAILBOX, &[3], &record, false, i64::MAX, u64::MAX)
        .is_err());
    assert_eq!(ack_ids(&store), [vec![1], vec![2]]);

    let (_, store) = fault_store();
    seed_ack(&store);
    lock(&store.messages)
        .unwrap()
        .execute_batch("PRAGMA `query_only` = ON")
        .unwrap();
    assert!(store
        .put_plain(&MAILBOX, &[3], &record, false, i64::MAX, u64::MAX)
        .is_err());
    assert_eq!(ack_ids(&store), [vec![1], vec![2]]);
}

#[test]
fn mailbox_ack_crash_rolls_back_on_reopen() {
    if let Ok(dir) = std::env::var(CRASH_DIR) {
        let store = BootstrapStore::new(&dir).unwrap();
        seed_ack(&store);
        {
            let db = lock(&store.messages).unwrap();
            fail_ack_delete(&db, 2);
        }
        assert!(store.ack_envelopes(&MAILBOX, &[vec![1], vec![2]]).is_err());
        std::process::abort();
    }

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/storage-faults")
        .join(uuid::Uuid::new_v4().to_string());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([CRASH_TEST, "--exact", "--test-threads=1"])
        .env(CRASH_DIR, &dir)
        .status()
        .unwrap();
    assert!(!status.success());
    let store = BootstrapStore::new(dir.to_str().unwrap()).unwrap();
    assert_eq!(ack_ids(&store), [vec![1], vec![2]]);
}
