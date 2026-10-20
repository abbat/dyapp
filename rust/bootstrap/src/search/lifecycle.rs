use super::tests::{assert_digest_consistent, fill, index, profile, put_at, signed, sized};
use super::*;
use dyapp_identity::Identity;

#[test]
fn versions_limits_and_tombstones() {
    let index = index();
    let id = Identity::generate();
    let (key, p1, r1) = signed(&id, profile(1));
    assert_eq!(index.put(&key, &p1, &r1, 1_000, true).unwrap(), Put::Stored);
    assert_eq!(index.put(&key, &p1, &r1, 2_000, true).unwrap(), Put::Stale);
    let (_, p2, r2) = signed(&id, profile(2));
    assert_eq!(
        index.put(&key, &p2, &r2, 1_100, true).unwrap(),
        Put::Limited { retry_after: 500 }
    );
    assert_eq!(index.put(&key, &p2, &r2, 1_600, true).unwrap(), Put::Stored);
    assert_eq!(index.records(&[key], 1 << 20).unwrap(), vec![r2.clone()]);
    let (_, p3, r3) = signed(&id, Profile::tombstone(3, 0, 0));
    assert_eq!(index.put(&key, &p3, &r3, 2_300, true).unwrap(), Put::Stored);
    assert!(index.records(&[key], 1 << 20).unwrap().is_empty());
    assert_eq!(index.versions(&[key]).unwrap(), vec![Some(3)]);
    assert_eq!(index.put(&key, &p2, &r2, 9_000, true).unwrap(), Put::Stale);
}

#[test]
fn stored_pow_difficulty_handles_missing_and_corrupt_records() {
    let index = index();
    let owner = Identity::generate();
    let mut profile = profile(1);
    profile.pow_bits = 7;
    let (key, profile, record) = signed(&owner, profile);
    assert_eq!(index.pow_bits(&key).unwrap(), None);
    index.put(&key, &profile, &record, 1000, true).unwrap();
    assert_eq!(index.pow_bits(&key).unwrap(), Some(7));
    for bytes in [
        vec![0xff],
        SignedRecord {
            payload: vec![0xff],
            ..Default::default()
        }
        .encode_to_vec(),
    ] {
        lock(&index.db)
            .unwrap()
            .execute(
                "UPDATE `record` SET `record` = ?1 WHERE `key` = ?2",
                params![bytes, &key[..]],
            )
            .unwrap();
        assert_eq!(index.pow_bits(&key).unwrap(), None);
    }
}

#[test]
fn heartbeats_two_per_day() {
    let index = index();
    let id = Identity::generate();
    let (key, p, r) = signed(&id, profile(4));
    let day = 86_400 * 20_000;
    assert_eq!(index.heartbeat(&key, 4, day).unwrap(), Beat::NotFound);
    index.put(&key, &p, &r, day, true).unwrap();
    assert_eq!(index.heartbeat(&key, 5, day).unwrap(), Beat::NotFound);
    assert_eq!(index.heartbeat(&key, 3, day).unwrap(), Beat::NotFound);
    assert_eq!(index.heartbeat(&key, 4, day + 10).unwrap(), Beat::Ok);
    assert_eq!(index.heartbeat(&key, 4, day + 20).unwrap(), Beat::Ok);
    assert_eq!(
        index.heartbeat(&key, 4, day + 30).unwrap(),
        Beat::Limited {
            retry_after: 86_400 - 30
        }
    );
    assert_eq!(index.heartbeat(&key, 4, day + 86_400).unwrap(), Beat::Ok);
}

#[test]
fn catch_up_skips_the_update_limit() {
    let index = index();
    let id = Identity::generate();
    let (key, p1, r1) = signed(&id, profile(1));
    let (_, p2, r2) = signed(&id, profile(2));
    index.put(&key, &p1, &r1, 1_000, false).unwrap();
    assert_eq!(
        index.put(&key, &p2, &r2, 1_001, false).unwrap(),
        Put::Stored
    );
    assert_eq!(index.put(&key, &p1, &r1, 1_002, false).unwrap(), Put::Stale);
}

#[test]
fn reader_works_during_a_write_transaction() {
    let index = index();
    let (key, p, r) = signed(&Identity::generate(), profile(1));
    index.put(&key, &p, &r, 1_000, true).unwrap();
    let mut db = lock(&index.db).unwrap();
    let tx = db.change().unwrap();
    tx.execute("DELETE FROM `record`", []).unwrap();
    // The reader sees the committed state.
    assert_eq!(index.records(&[key], 1 << 20).unwrap(), vec![r]);
    drop(tx);
}

#[test]
fn puts_past_max_bytes_stay_near_it() {
    let probe = index();
    put_at(&probe, "2950159", 2_000, probe.created(), false);
    let max = probe.usage().unwrap();
    let index = sized(max);
    put_at(&index, "2950159", 4_000, index.created(), false);
    // Usage is re-read every 1 % of max_bytes of records written, but the file also grows by
    // their index rows and random keys fill B-tree pages unevenly: 2 % was flaky.
    assert!(index.usage().unwrap() <= max * 105 / 100);
    assert!(index.place_counts()[0].2 < 4_000);
    assert_eq!(index.place_counts(), index.recount());
    assert_digest_consistent(&index);
}

#[test]
fn expiry_and_inventory() {
    let index = index();
    let keys: Vec<_> = (0..40)
        .map(|i| {
            let (key, p, r) = signed(&Identity::generate(), profile(1));
            index.put(&key, &p, &r, i * 100, true).unwrap();
            key
        })
        .collect();
    let all: usize = (0..16)
        .map(|t| index.inventory(t, &[], &[], 10_000).unwrap().len())
        .sum();
    assert_eq!(all, 40);
    let t = topic(&keys[0]);
    let listed = index.inventory(t, &[], &[], 10_000).unwrap();
    assert!(listed.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(listed.iter().all(|(k, _)| topic(k) == t));
    let after = index.inventory(t, &listed[0].0, &[], 10_000).unwrap();
    assert_eq!(after.len(), listed.len() - 1);
    // Heartbeats 0..900 are before 1 000.
    assert_eq!(index.expire(1_000).unwrap(), 10);
    assert_eq!(index.versions(&keys[..10]).unwrap(), vec![None; 10]);
    assert!(index
        .versions(&keys[10..])
        .unwrap()
        .iter()
        .all(Option::is_some));
}

#[test]
fn expire_is_capped_per_call() {
    let index = index();
    let cap = usize::try_from(BATCH * EXPIRE_BATCHES).unwrap();
    fill(&index, u64::try_from(cap).unwrap() + 10, 0);
    assert_eq!(index.expire(i64::MAX).unwrap(), cap);
    assert_eq!(index.expire(i64::MAX).unwrap(), 10);
}
