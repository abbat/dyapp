//! Newcomer quota and size eviction tests.
use super::tests::{index, profile, put_at, quota, signed, sized, Counts};
use super::*;
use dyapp_identity::Identity;

const DAY: i64 = 86_400;

/// The share of a 1 TiB file that holds `n` records.
fn records(n: u64) -> f64 {
    let one = signed(&Identity::generate(), profile(1)).2.encoded_len() as u64;
    (n * one) as f64 / (1u64 << 40) as f64
}

fn held(index: &SearchIndex, keys: &[[u8; 32]]) -> usize {
    index.versions(keys).unwrap().iter().flatten().count()
}

fn n(counts: &Counts, place: &str) -> i64 {
    counts.iter().find(|c| c.1 == place).map_or(0, |c| c.2)
}

#[test]
fn newcomers_past_the_quota_evict_the_oldest_heartbeat() {
    // The file is never full: only the quota for 20 records evicts.
    let index = quota(1 << 40, records(20));
    let late = index.created() + 31 * DAY;
    let first = put_at(&index, "2950159", 20, late, true);
    let more = put_at(&index, "2950159", 5, late + 100, true);
    assert_eq!(held(&index, &more), 5);
    assert_eq!(held(&index, &first[..5]), 0);
    assert_eq!(held(&index, &first[5..]), 15);
    assert_eq!(index.take_newcomer_evictions(), 5);
    assert_eq!(index.take_newcomer_evictions(), 0);
}

#[test]
fn newcomers_graduate_after_newcomer_days() {
    let index = quota(1 << 40, records(10));
    let t = index.created() + 31 * DAY;
    let old = put_at(&index, "2950159", 10, t, true);
    // Thirty days after the last of them all ten are established; ten newcomers fit beside.
    put_at(&index, "2950159", 10, t + 30 * DAY + 10, true);
    assert_eq!(held(&index, &old), 10);
    assert_eq!(index.take_newcomer_evictions(), 0);
}

#[test]
fn grace_covers_only_replicas_in_the_first_days() {
    let index = index();
    let t = index.created() + 1;
    let first_seen = |limit: bool, now: i64| -> i64 {
        let key = put_at(&index, "", 1, now, limit)[0];
        index.column("SELECT `first_seen` FROM `key` WHERE `key` = ?", &key)
    };
    assert_eq!(first_seen(false, t), 0);
    assert_eq!(first_seen(true, t), t);
    assert_eq!(first_seen(false, t + 31 * DAY), t + 31 * DAY);
}

#[test]
fn water_filling_spares_small_places() {
    let ids: Vec<String> = crate::places::of("DE")
        .iter()
        .take(101)
        .map(u32::to_string)
        .collect();
    let (big, small) = (&ids[0], &ids[1..]);
    let fill = |index: &SearchIndex, big_keys: usize| {
        for place in small {
            put_at(index, place, 3, index.created() + 1, false);
        }
        put_at(index, big, big_keys, index.created() + 1, false);
    };
    let probe = index();
    fill(&probe, 300);
    let index = sized(probe.usage().unwrap());
    fill(&index, 900);
    let counts = index.place_counts();
    assert!(small.iter().all(|place| n(&counts, place) == 3));
    assert!(n(&counts, big) < 900);
    // Usage is re-read every 1 % of max_bytes of records written; the file also grows by
    // their index rows, which weighs more in a file this small.
    assert!(index.usage().unwrap() <= probe.usage().unwrap() * 105 / 100);
}

#[test]
fn the_larger_country_bucket_goes_first() {
    let fill = |index: &SearchIndex| {
        put_at(index, "", 400, index.created() + 1, false);
        put_at(index, "2950159", 200, index.created() + 1, false);
    };
    let probe = index();
    fill(&probe);
    let index = sized(probe.usage().unwrap() * 9 / 10);
    fill(&index);
    let counts = index.place_counts();
    assert_eq!(n(&counts, "2950159"), 200);
    assert!(n(&counts, "") < 400);
}

#[test]
fn size_eviction_keeps_tombstones_and_newcomers() {
    let id = Identity::generate();
    let (key, tomb, rt) = signed(&id, Profile::tombstone(1, 0, 0));
    let probe = index();
    probe
        .put(&key, &tomb, &rt, probe.created() + 1, false)
        .unwrap();
    put_at(&probe, "", 300, probe.created() + 1, false);
    let index = sized(probe.usage().unwrap());
    let t = index.created() + 1;
    index.put(&key, &tomb, &rt, t, false).unwrap();
    // Five newcomers, well under the quota, then established keys overflow the file.
    let newcomers = put_at(&index, "", 5, t, true);
    put_at(&index, "", 330, t, false);
    assert_eq!(index.versions(&[key]).unwrap(), vec![Some(1)]);
    assert_eq!(held(&index, &newcomers), 5);
    assert_eq!(index.take_newcomer_evictions(), 0);
}

#[test]
fn eviction_falls_back_past_newcomer_places() {
    let ids = crate::places::of("DE");
    let fill = |index: &SearchIndex| {
        let t = index.created() + 1;
        let established = put_at(index, &ids[0].to_string(), 2, t, false);
        let newcomers: Vec<_> = ids[1..18]
            .iter()
            .flat_map(|place| put_at(index, &place.to_string(), 3, t, true))
            .collect();
        (established, newcomers)
    };
    let probe = index();
    fill(&probe);
    let index = quota(probe.usage().unwrap() * 9 / 10, 1.0);
    let (established, newcomers) = fill(&index);
    assert!(held(&index, &established) < 2);
    assert_eq!(held(&index, &newcomers), 51);
}
