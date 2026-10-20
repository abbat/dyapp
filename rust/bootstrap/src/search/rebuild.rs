//! Candidate generation build tests.
use super::tests::{assert_digest_consistent, fill, index, profile, signed};
use super::*;
use crate::maintenance::RebuildBudget;
use dyapp_identity::Identity;
use rusqlite::types::Value;

/// Every row of the tables a generation holds, in key order.
fn dump(db: &Connection) -> Vec<Vec<Value>> {
    let mut all = vec![];
    for table in [
        "key",
        "profile",
        "interest",
        "record",
        "place_count",
        "digest",
    ] {
        let mut statement = db
            .prepare(&format!("SELECT * FROM `{table}` ORDER BY 1, 2"))
            .unwrap();
        let n = statement.column_count();
        let rows = statement
            .query_map([], |row| (0..n).map(|i| row.get(i)).collect())
            .unwrap();
        all.extend(rows.map(|row| row.unwrap()));
    }
    all
}

/// Marks `keys` changed, as a mutation would.
fn mark(index: &SearchIndex, keys: impl IntoIterator<Item = [u8; 32]>) {
    let mut db = lock(&index.db).unwrap();
    let mut tx = db.change().unwrap();
    keys.into_iter().for_each(|key| tx.mark(key));
    tx.commit().unwrap();
}

fn fake(i: u32) -> [u8; 32] {
    let mut key = [0xff; 32];
    key[28..].copy_from_slice(&i.to_be_bytes());
    key
}

#[tokio::test]
async fn changes_during_the_copy_reach_the_candidate() {
    let index = index();
    let t = index.created();
    let keys = fill(&index, 600, t);
    let id = Identity::generate();
    let (gone, p1, r1) = signed(&id, profile(1));
    index.put(&gone, &p1, &r1, t, true).unwrap();
    let budget = RebuildBudget::new(64);
    // The first batch is copied before the changes run, the other two after.
    let changes = async {
        let (_, tomb, rt) = signed(&id, Profile::tombstone(2, 0, 0));
        let at = t + UPDATE_SECONDS;
        assert_eq!(index.put(&gone, &tomb, &rt, at, true).unwrap(), Put::Stored);
        assert_eq!(index.heartbeat(&keys[300], 1, at).unwrap(), Beat::Ok);
        fill(&index, 5, at);
        assert_eq!(index.expire(t + 10).unwrap(), 10);
    };
    let (built, ()) = tokio::join!(index.build(2, &budget, 0), changes);
    let candidate = built.unwrap();
    assert_eq!(candidate.version, 2);
    assert!(candidate.path.ends_with("profiles-idx-v2.db"));
    let copy = Connection::open(&candidate.path).unwrap();
    let source = dump(&lock(&index.db).unwrap());
    assert_eq!(dump(&copy), source);
    assert_eq!(
        source.iter().filter(|row| row.len() == 10).count(),
        600 + 1 + 5 - 10
    );
    assert_digest_consistent(&index);
    let generation: (u32, bool) = copy
        .query_row("SELECT * FROM `generation`", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(generation, (2, false));
    let created: i64 = copy
        .query_row(
            "SELECT `value` FROM `meta` WHERE `name` = 'created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(created, t);
}

#[tokio::test]
async fn overflow_aborts_and_deletes_the_candidate() {
    let index = index();
    fill(&index, 300, index.created());
    let budget = RebuildBudget::new(64);
    let flood = async { mark(&index, (0..=writer::DIRTY_KEYS as u32).map(fake)) };
    let (built, ()) = tokio::join!(index.build(2, &budget, 0), flood);
    assert!(matches!(built, Err(Abort::Overflow)));
    assert!(!index.generation_path(2).exists());
    // Tracking stopped: nothing is collected for a candidate that is gone.
    fill(&index, 1, 0);
    assert_eq!(index.take_dirty().unwrap(), Ok(vec![]));
}

#[tokio::test]
async fn low_disk_aborts_before_the_copy() {
    let index = index();
    fill(&index, 3, index.created());
    let built = index.build(2, &RebuildBudget::new(64), u64::MAX).await;
    assert!(matches!(built, Err(Abort::Disk)));
    assert!(!index.generation_path(2).exists());
}

#[tokio::test]
async fn continuous_changes_postpone_then_replay_settles() {
    let index = index();
    fill(&index, 3, index.created());
    let budget = RebuildBudget::new(64);
    // More distinct keys than a settled round may take, changed again during every batch.
    let load = async {
        loop {
            mark(&index, (0..1100).map(fake));
            tokio::task::yield_now().await;
        }
    };
    let built = tokio::select! {
        built = index.build(2, &budget, 0) => built,
        () = load => unreachable!(),
    };
    let Err(Abort::Postpone(candidate)) = built else {
        panic!("expected a postponed build, got {built:?}");
    };
    assert!(candidate.path.exists());
    let candidate = index.replay(candidate, &budget, 0).await.unwrap();
    let copy = Connection::open(&candidate.path).unwrap();
    assert_eq!(dump(&copy), dump(&lock(&index.db).unwrap()));
}
