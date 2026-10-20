//! Generation start, activation and forced rebuild tests.
use super::*;
use crate::maintenance::RebuildBudget;
use crate::search::tests::{fill, index};
use std::time::Duration;

const NOW: [Duration; 2] = [Duration::ZERO; 2];

fn reopen(base: &Path, target: u32) -> Result<SearchIndex> {
    SearchIndex::open_target(base, &crate::config::Search::default(), target)
}

/// An index at `target` 2 over a filled v1, and its v1 path.
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
fn a_second_open_of_the_directory_is_refused() {
    let index = index();
    let error = reopen(&index.path, 1).err().unwrap().to_string();
    assert!(error.contains("search.lock"), "{error}");
}

#[tokio::test]
async fn rebuild_switches_and_restart_keeps_the_highest_complete_generation() {
    let (base, keys) = filled(300);
    let index = reopen(&base, 2).unwrap();
    assert_eq!(index.index_version(), 1);
    let stale = index.reader().unwrap();
    index.rebuild_with(&RebuildBudget::new(64), 0, NOW).await;
    assert_eq!(index.index_version(), 2);
    assert_eq!(index.rebuild_report().phase, "serving");
    assert!(!base.exists());
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

    // A crash left an incomplete v3: it is deleted, v2 stays active.
    let v3 = generation_file(&base, 3);
    let db = storage::open(&v3, SCHEMA, STEPS).unwrap();
    stamp(&db.into_inner().unwrap(), 3, false).unwrap();
    let index = reopen(&base, 3).unwrap();
    assert_eq!(index.index_version(), 2);
    assert!(!v3.exists());
    drop(index);

    // An older binary refuses a newer generation and names its file.
    let error = reopen(&base, 1).err().unwrap().to_string();
    assert!(error.contains("profiles-idx-v2.db"), "{error}");
}

#[tokio::test]
async fn activation_replays_settled_keys_or_postpones() {
    let (base, _) = filled(10);
    let index = reopen(&base, 2).unwrap();
    let candidate = index.build(2, &RebuildBudget::new(64), 0).await.unwrap();
    mark(&index, 0..(candidate::SETTLED_KEYS as u32 + 1));
    let Err(Abort::Postpone(candidate)) = index.activate(candidate, Replay::Settled) else {
        panic!("expected a postponed activation");
    };
    assert_eq!(index.index_version(), 1);
    index.activate(candidate, Replay::All).unwrap();
    assert_eq!(index.index_version(), 2);
    assert_eq!(index.take_dirty().unwrap(), Ok(vec![]));
}

#[tokio::test]
async fn three_disk_aborts_drop_the_index_for_a_refill() {
    let (base, keys) = filled(5);
    let index = reopen(&base, 2).unwrap();
    let budget = RebuildBudget::new(64);
    index.rebuild_with(&budget, u64::MAX, NOW).await;
    assert_eq!(index.index_version(), 2);
    assert!(!base.exists());
    let report = index.rebuild_report();
    assert_eq!((report.phase, report.drop_refills), ("refilling", 1));
    assert!(index.versions(&keys).unwrap().iter().all(Option::is_none));
    assert!(index.query(&[], 10, 10_000).unwrap().unwrap().partial);
    drop(index);

    // Refilling outlives a restart and ends once.
    let index = reopen(&base, 2).unwrap();
    assert!(index.refilling());
    index.refilled().unwrap();
    assert!(!index.query(&[], 10, 10_000).unwrap().unwrap().partial);
    drop(index);
    assert!(!reopen(&base, 2).unwrap().refilling());
}
