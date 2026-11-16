//! Generation start, activation and forced rebuild tests.
use super::*;
use crate::maintenance::RebuildBudget;
use crate::search::candidate;
use crate::search::tests::{fill, index};
use crate::search::Put;
use dyapp_profile::Profile;
use prost::Message as _;
use std::time::Duration;

const NOW: [Duration; 2] = [Duration::ZERO; 2];
/// The binary's index version and the next one a rebuild goes to.
const V: u32 = INDEX_VERSION;
const NEXT: u32 = INDEX_VERSION + 1;

fn reopen(base: &Path, target: u32) -> Result<SearchIndex> {
    SearchIndex::open_target(base, &crate::config::Search::default(), target)
}

/// A filled index at `INDEX_VERSION`, and its base path.
fn filled(n: u64) -> (PathBuf, Vec<[u8; 32]>) {
    let index = index();
    let keys = fill(&index, n, index.created());
    (index.path.clone(), keys)
}

fn mark(index: &SearchIndex, keys: impl IntoIterator<Item = u32>) {
    let mut db = lock(&index.db).unwrap();
    let mut tx = db.change().unwrap();
    for i in keys {
        let mut key = [0xff; 32];
        key[28..].copy_from_slice(&i.to_be_bytes());
        tx.mark(key);
    }
    tx.commit().unwrap();
}

#[test]
fn reader_retries_one_failed_open_and_reuses_the_pooled_connection() {
    use std::cell::Cell;
    use std::rc::Rc;

    let index = index();
    let active = index.active_path();
    let backup = active.with_extension("retry-backup");
    std::fs::rename(&active, &backup).unwrap();
    let attempts = Rc::new(Cell::new(0));
    let observed = Rc::clone(&attempts);
    let _hook = fail_reader_opens(move || {
        observed.set(observed.get() + 1);
        if observed.get() == 1 {
            std::fs::rename(&backup, &active).unwrap();
            return true;
        }
        false
    });
    let reader = index.reader().unwrap();
    assert_eq!(attempts.get(), 2);
    drop(reader);
    assert_eq!(lock(&index.readers).unwrap().conns.len(), 1);
    let pooled = index.reader().unwrap();
    assert_eq!(attempts.get(), 2);
    drop(pooled);
    assert_eq!(lock(&index.readers).unwrap().conns.len(), 1);
}

#[test]
fn reader_returns_an_error_after_a_persistent_open_failure() {
    use std::cell::Cell;
    use std::rc::Rc;

    let index = index();
    let active = index.active_path();
    let backup = active.with_extension("failed-backup");
    std::fs::rename(&active, &backup).unwrap();
    let attempts = Rc::new(Cell::new(0));
    let observed = Rc::clone(&attempts);
    {
        let _hook = fail_reader_opens(move || {
            observed.set(observed.get() + 1);
            assert!(observed.get() <= 2, "reader retried more than once");
            true
        });
        let error = index.reader().err().unwrap().to_string();
        assert!(error.contains("injected reader open failure"), "{error}");
        assert_eq!(attempts.get(), 2);
        assert!(lock(&index.readers).unwrap().conns.is_empty());
    }
    std::fs::rename(backup, active).unwrap();
    drop(index.reader().unwrap());
    assert_eq!(lock(&index.readers).unwrap().conns.len(), 1);
}

#[test]
fn a_second_open_of_the_directory_is_refused() {
    let index = index();
    let error = reopen(&index.path, 1).err().unwrap().to_string();
    assert!(error.contains("search.lock"), "{error}");
}

#[tokio::test]
async fn rebuild_switches_and_restart_keeps_the_highest_complete_generation() {
    let (base, keys) = filled(300);
    let index = reopen(&base, NEXT).unwrap();
    assert_eq!(index.index_version(), V);
    let pooled = index.reader().unwrap();
    drop(pooled);
    assert_eq!(lock(&index.readers).unwrap().conns.len(), 1);
    let stale = index.reader().unwrap();
    index.rebuild_with(&RebuildBudget::new(64), 0, NOW).await;
    assert_eq!(index.index_version(), NEXT);
    assert_eq!(index.rebuild_report().phase, "serving");
    assert!(!generation_file(&base, V).exists());
    // A reader of the old generation is not pooled again.
    drop(stale);
    assert!(lock(&index.readers).unwrap().conns.is_empty());
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_some));
    let reader = index.reader().unwrap();
    let count: i64 = reader
        .query_row("SELECT COUNT(*) FROM `key`", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 300);
    drop(reader);
    drop(index);

    // A crash left an incomplete later generation: it is deleted, NEXT stays active.
    let later = generation_file(&base, NEXT + 1);
    let db = storage::open(&later, &[SCHEMA]).unwrap();
    stamp(&db.into_inner().unwrap(), NEXT + 1, false).unwrap();
    let index = reopen(&base, NEXT + 1).unwrap();
    assert_eq!(index.index_version(), NEXT);
    assert!(!later.exists());
    drop(index);

    // An older binary refuses a newer generation and names its file.
    let error = reopen(&base, V).err().unwrap().to_string();
    assert!(
        error.contains(&format!("profiles-idx-v{NEXT}.db")),
        "{error}"
    );
}

#[test]
fn open_keeps_only_the_highest_complete_generation() {
    let (base, _) = filled(1);
    let later = generation_file(&base, NEXT);
    let db = storage::open(&later, &[SCHEMA]).unwrap();
    stamp(&db.into_inner().unwrap(), NEXT, true).unwrap();
    let index = reopen(&base, NEXT).unwrap();
    assert_eq!(index.index_version(), NEXT);
    assert!(!base.exists());
}

#[test]
fn a_fresh_target_generation_is_stamped_complete() {
    let (base, _) = filled(1);
    remove_files(&base);
    let index = reopen(&base, NEXT).unwrap();
    let complete: bool = index
        .reader()
        .unwrap()
        .query_row("SELECT `complete` FROM `generation`", [], |row| row.get(0))
        .unwrap();
    assert!(complete);
}

#[tokio::test]
async fn activation_at_the_exact_size_limit_is_not_over() {
    let (base, _) = filled(1);
    let mut index = reopen(&base, NEXT).unwrap();
    let candidate = index.build(NEXT, &RebuildBudget::new(64), 0).await.unwrap();
    let conn = Connection::open(&candidate.path).unwrap();
    index.max_bytes = storage::used(&conn).unwrap();
    drop(conn);
    index.activate(candidate, Replay::All).unwrap();
    assert!(!index.over.load(Ordering::Relaxed));
}

#[tokio::test]
async fn withdrawal_proof_survives_generation_rebuild_without_a_visible_profile() {
    let owner = dyapp_identity::Identity::from_secret(&[7; 32]);
    let profile = Profile::search_tombstone(2, 0, 0);
    let record = profile.sign(&owner);
    let key = dyapp_identity::key_hash(&owner.public_key());
    let index = index();
    assert_eq!(
        index
            .put(&key, &profile, &record, index.created(), false)
            .unwrap(),
        Put::Stored
    );
    assert_eq!(
        index.records(&[key], usize::MAX).unwrap(),
        vec![record.clone()]
    );
    let bytes: i64 = index
        .reader()
        .unwrap()
        .query_row(
            "SELECT `bytes` FROM `key` WHERE `key` = ?",
            [&key[..]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(usize::try_from(bytes).unwrap(), record.encoded_len());
    let base = index.path.clone();
    drop(index);
    let index = reopen(&base, NEXT).unwrap();
    index.rebuild_with(&RebuildBudget::new(64), 0, NOW).await;
    assert_eq!(index.index_version(), NEXT);
    assert_eq!(index.records(&[key], usize::MAX).unwrap(), vec![record]);
    let visible: i64 = index
        .reader()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM `profile`", [], |row| row.get(0))
        .unwrap();
    assert_eq!(visible, 0);
}

#[tokio::test]
async fn activation_replays_settled_keys_or_postpones() {
    let (base, _) = filled(10);
    let index = reopen(&base, NEXT).unwrap();
    let candidate = index.build(NEXT, &RebuildBudget::new(64), 0).await.unwrap();
    mark(&index, 0..=u32::try_from(candidate::SETTLED_KEYS).unwrap());
    let Err(Abort::Postpone(candidate)) = index.activate(candidate, Replay::Settled) else {
        panic!("expected a postponed activation");
    };
    assert_eq!(index.index_version(), V);
    index.activate(candidate, Replay::All).unwrap();
    assert_eq!(index.index_version(), NEXT);
    assert_eq!(index.take_dirty().unwrap(), Ok(vec![]));
}

#[tokio::test]
async fn activation_replay_failure_keeps_the_serving_generation_after_reopen() {
    let (base, keys) = filled(5);
    let index = reopen(&base, NEXT).unwrap();
    let candidate = index.build(NEXT, &RebuildBudget::new(64), 0).await.unwrap();
    let candidate_path = candidate.path.clone();
    let extra = fill(&index, 1, index.created());
    Connection::open(&candidate.path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_replay` BEFORE INSERT ON `profile`
             BEGIN SELECT RAISE(ABORT, 'injected replay failure'); END;",
        )
        .unwrap();
    let Err(Abort::Io(_)) = index.activate(candidate, Replay::All) else {
        panic!("expected replay failure");
    };
    assert_eq!(index.index_version(), V);
    assert!(!candidate_path.exists());
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_some));
    assert!(index.versions(&extra).unwrap().iter().all(Option::is_some));
    drop(index);

    let index = reopen(&base, NEXT).unwrap();
    assert_eq!(index.index_version(), V);
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_some));
    assert!(index.versions(&extra).unwrap().iter().all(Option::is_some));
}

#[tokio::test]
async fn activation_commit_failure_keeps_the_serving_generation_after_reopen() {
    let (base, keys) = filled(5);
    let index = reopen(&base, NEXT).unwrap();
    let candidate = index.build(NEXT, &RebuildBudget::new(64), 0).await.unwrap();
    let candidate_path = candidate.path.clone();
    Connection::open(&candidate.path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER `fail_complete` BEFORE UPDATE OF `complete` ON `generation`
             BEGIN SELECT RAISE(ABORT, 'injected commit failure'); END;",
        )
        .unwrap();
    let Err(Abort::Io(_)) = index.activate(candidate, Replay::All) else {
        panic!("expected activation failure");
    };
    assert_eq!(index.index_version(), V);
    assert!(!candidate_path.exists());
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_some));
    drop(index);

    let index = reopen(&base, NEXT).unwrap();
    assert_eq!(index.index_version(), V);
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_some));
}

#[tokio::test]
async fn three_disk_aborts_drop_the_index_for_a_refill() {
    let (base, keys) = filled(5);
    let index = reopen(&base, NEXT).unwrap();
    let budget = RebuildBudget::new(64);
    index.rebuild_with(&budget, u64::MAX, NOW).await;
    assert_eq!(index.index_version(), NEXT);
    assert!(!generation_file(&base, V).exists());
    let report = index.rebuild_report();
    assert_eq!((report.phase, report.drop_refills), ("refilling", 1));
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_none));
    assert!(index.query(&[], 10, 10_000).unwrap().unwrap().partial);
    drop(index);

    // Refilling outlives a restart and ends once.
    let index = reopen(&base, NEXT).unwrap();
    assert!(index.refilling());
    index.refilled().unwrap();
    assert!(!index.query(&[], 10, 10_000).unwrap().unwrap().partial);
    drop(index);
    assert!(!reopen(&base, NEXT).unwrap().refilling());
}
