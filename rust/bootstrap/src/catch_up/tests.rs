use super::*;

fn answer(hashes: &[[u8; 32]]) -> DigestResponse {
    DigestResponse {
        status: Status::Ok.into(),
        hashes: hashes.iter().map(|h| h.to_vec()).collect(),
    }
}

fn ranges(asks: &[Ask]) -> Vec<(Vec<u8>, Vec<u8>)> {
    asks.iter()
        .filter_map(|a| match a {
            Ask::Range(r) => Some((r.after.clone(), r.before.clone())),
            Ask::Digest { .. } => None,
        })
        .collect()
}

/// Bucket 7 of topic 3 as one inventory range.
fn bucket_7() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![(
        decrement(&slot_start(3, 7 << 8)),
        slot_start(3, 8 << 8).to_vec(),
    )]
}

#[test]
fn equal_digest_asks_nothing() {
    let local = vec![[1u8; 32]; 256];
    assert!(on_digest(&local, 3, None, Some(&answer(&local))).is_empty());
    assert!(on_digest(&local, 3, Some(7), Some(&answer(&local))).is_empty());
}

#[test]
fn differing_bucket_asks_its_sub_digest() {
    let local = vec![[0u8; 32]; 256];
    let mut remote = local.clone();
    remote[7] = [9; 32];
    let asks = on_digest(&local, 3, None, Some(&answer(&remote)));
    assert!(matches!(
        asks.as_slice(),
        [Ask::Digest {
            topic: 3,
            bucket: Some(7)
        }]
    ));
}

#[test]
fn differing_sub_buckets_coalesce_into_ranges() {
    let local = vec![[0u8; 32]; 256];
    let mut remote = local.clone();
    remote[4] = [1; 32];
    remote[5] = [1; 32];
    remote[255] = [1; 32];
    let asks = on_digest(&local, 3, Some(7), Some(&answer(&remote)));
    let p = |s: u32| (7 << 8) | s;
    let below = |s| decrement(&slot_start(3, p(s)));
    assert_eq!(
        ranges(&asks),
        vec![
            (below(4), slot_start(3, p(6)).to_vec()),
            (below(255), slot_start(3, p(0) + 256).to_vec()),
        ]
    );
}

#[test]
fn range_bounds_hold_exactly_the_slots() {
    let (from, to) = (0x0705, 0x0707);
    let r = range(3, from, to);
    for p in [from - 1, from, to - 1, to] {
        let start = slot_start(3, p);
        let inside = start[..] > r.after[..] && start[..] < r.before[..];
        assert_eq!(inside, (from..to).contains(&p), "slot {p:#x}");
    }
}

#[test]
fn last_slot_has_no_upper_bound_and_first_no_lower() {
    let local = vec![[0u8; 32]; 256];
    let mut remote = local.clone();
    remote[0] = [1; 32];
    remote[255] = [1; 32];
    let first = on_digest(&local, 0, Some(0), Some(&answer(&remote)));
    assert_eq!(ranges(&first)[0].0, Vec::<u8>::new());
    let last = on_digest(&local, 0, Some(255), Some(&answer(&remote)));
    assert_eq!(ranges(&last)[1].1, Vec::<u8>::new());
}

#[test]
fn any_failure_falls_back() {
    let local = vec![[0u8; 32]; 256];
    let short = DigestResponse {
        status: Status::Ok.into(),
        hashes: vec![vec![0; 32]; 255],
    };
    let narrow = DigestResponse {
        status: Status::Ok.into(),
        hashes: vec![vec![0; 31]; 256],
    };
    let refused = DigestResponse {
        status: Status::Refused.into(),
        hashes: vec![],
    };
    for bad in [None, Some(&short), Some(&narrow), Some(&refused)] {
        assert_eq!(
            ranges(&on_digest(&local, 3, None, bad)),
            vec![(decrement(&slot_start(3, 0)), vec![])]
        );
        assert_eq!(ranges(&on_digest(&local, 3, Some(7), bad)), bucket_7());
    }
}

#[test]
fn many_differences_fetch_the_whole_range() {
    let local = vec![[0u8; 32]; 256];
    let whole = vec![(decrement(&slot_start(3, 0)), Vec::new())];
    let mut remote = local.clone();
    for hash in remote.iter_mut().take(MAX_BUCKETS + 1) {
        *hash = [1; 32];
    }
    assert_eq!(
        ranges(&on_digest(&local, 3, None, Some(&answer(&remote)))),
        whole
    );
    // At the limit the answer is still per bucket.
    remote[MAX_BUCKETS] = [0; 32];
    assert_eq!(
        on_digest(&local, 3, None, Some(&answer(&remote))).len(),
        MAX_BUCKETS
    );
    // MAX_RUNS + 1 separate runs: every other sub-bucket differs.
    let mut remote = local.clone();
    for i in 0..=MAX_RUNS {
        remote[2 * i] = [1; 32];
    }
    assert_eq!(
        ranges(&on_digest(&local, 3, Some(7), Some(&answer(&remote)))),
        bucket_7()
    );
    remote[2 * MAX_RUNS] = [0; 32];
    assert_eq!(
        on_digest(&local, 3, Some(7), Some(&answer(&remote))).len(),
        MAX_RUNS
    );
}

#[test]
fn inventory_pages_and_drops_past_before() {
    let range = Range {
        topic: 0,
        after: vec![],
        before: vec![5; 32],
    };
    let page: Vec<_> = (0..PAGE)
        .map(|i| {
            let mut k = [0u8; 32];
            k[1..3].copy_from_slice(&(i as u16).to_be_bytes());
            (k, 1)
        })
        .collect();
    let last = page[PAGE - 1].0;
    let (kept, next) = on_inventory(&range, page);
    assert_eq!(kept.len(), PAGE);
    let next = next.unwrap();
    assert_eq!((next.after, next.before), (last.to_vec(), vec![5; 32]));
    // An old peer ignores `before`: keys past it are dropped and paging stops.
    let (kept, next) = on_inventory(&range, vec![([1; 32], 1), ([5; 32], 1), ([6; 32], 1)]);
    assert_eq!(kept, vec![([1; 32], 1)]);
    assert!(next.is_none());
    // A short page ends the range.
    assert!(on_inventory(&range, vec![([1; 32], 1)]).1.is_none());
}
