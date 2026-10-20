//! Changed-key tracking tests.
use super::tests::{fill, index, profile, put_at, quota, signed, sized};
use super::*;
use dyapp_identity::Identity;
use std::collections::HashSet;

fn dirty(index: &SearchIndex) -> HashSet<[u8; 32]> {
    index.take_dirty().unwrap().unwrap().into_iter().collect()
}

fn removed(index: &SearchIndex, keys: &[[u8; 32]]) -> HashSet<[u8; 32]> {
    let versions = index.versions(keys).unwrap();
    keys.iter()
        .zip(versions)
        .filter_map(|(key, version)| version.is_none().then_some(*key))
        .collect()
}

#[test]
fn every_mutation_marks_its_key() {
    let index = index();
    let id = Identity::generate();
    let (key, p1, r1) = signed(&id, profile(1));
    index.put(&key, &p1, &r1, 0, true).unwrap();
    assert!(dirty(&index).is_empty());
    index.track_start().unwrap();
    // Stale and rate-limited puts and refused heartbeats change nothing.
    assert_eq!(index.put(&key, &p1, &r1, 1, true).unwrap(), Put::Stale);
    let (_, p2, r2) = signed(&id, profile(2));
    let limited = index.put(&key, &p2, &r2, 1, true).unwrap();
    assert!(matches!(limited, Put::Limited { .. }));
    assert_eq!(index.heartbeat(&key, 9, 1).unwrap(), Beat::NotFound);
    assert!(dirty(&index).is_empty());
    assert_eq!(index.heartbeat(&key, 1, 2).unwrap(), Beat::Ok);
    assert_eq!(dirty(&index), HashSet::from([key]));
    let at = UPDATE_SECONDS;
    assert_eq!(index.put(&key, &p2, &r2, at, true).unwrap(), Put::Stored);
    let (_, tomb, rt) = signed(&id, Profile::tombstone(3, 0, 0));
    assert_eq!(
        index.put(&key, &tomb, &rt, 2 * at, true).unwrap(),
        Put::Stored
    );
    // Repeated changes of one key are taken once.
    assert_eq!(index.take_dirty().unwrap(), Ok(vec![key]));
    let others = fill(&index, 3, 3 * at);
    assert_eq!(index.expire(i64::MAX).unwrap(), 4);
    let mut all = HashSet::from([key]);
    all.extend(others);
    assert_eq!(dirty(&index), all);
    index.track_stop().unwrap();
    fill(&index, 1, 0);
    assert!(dirty(&index).is_empty());
}

#[test]
fn newcomer_eviction_marks_the_evicted_key() {
    let one = signed(&Identity::generate(), profile(1)).2.encoded_len() as f64;
    let index = quota(1 << 40, 2.0 * one / (1u64 << 40) as f64);
    let late = index.created() + 31 * 86_400;
    let first = put_at(&index, "2950159", 2, late, true);
    index.track_start().unwrap();
    let more = put_at(&index, "2950159", 1, late + 100, true);
    assert_eq!(removed(&index, &first), HashSet::from([first[0]]));
    assert_eq!(dirty(&index), HashSet::from([first[0], more[0]]));
}

#[test]
fn size_eviction_marks_the_evicted_keys() {
    let probe = index();
    put_at(&probe, "2950159", 300, probe.created(), false);
    let index = sized(probe.usage().unwrap());
    let t = index.created();
    let before = put_at(&index, "2950159", 300, t, false);
    let held: Vec<_> = before
        .iter()
        .filter(|key| !removed(&index, &before).contains(*key))
        .copied()
        .collect();
    index.track_start().unwrap();
    let after = put_at(&index, "2950159", 60, t, false);
    // Eviction takes the highest `r`, which may fall on new keys only; those are in `after`.
    let evicted = removed(&index, &held);
    assert!(!evicted.is_empty() || !removed(&index, &after).is_empty());
    let mut expected: HashSet<_> = after.into_iter().collect();
    expected.extend(evicted);
    assert_eq!(dirty(&index), expected);
}

#[test]
fn overflow_loses_the_set_until_tracking_restarts() {
    let index = index();
    index.track_start().unwrap();
    {
        let mut db = lock(&index.db).unwrap();
        let mut tx = db.change().unwrap();
        for i in 0..=u32::try_from(writer::DIRTY_KEYS).unwrap() {
            let mut key = [0; 32];
            key[..4].copy_from_slice(&i.to_be_bytes());
            tx.mark(key);
        }
        tx.commit().unwrap();
    }
    assert_eq!(index.take_dirty().unwrap(), Err(Overflow));
    fill(&index, 1, 0);
    assert_eq!(index.take_dirty().unwrap(), Err(Overflow));
    index.track_start().unwrap();
    let key = fill(&index, 1, 0);
    assert_eq!(index.take_dirty().unwrap(), Ok(key));
}
