use super::*;

#[test]
fn cursor_wraps_with_a_256_hash_window_and_rolling_hour() {
    let hashes = (0u32..600)
        .map(|i| i.to_le_bytes().to_vec())
        .collect::<Vec<_>>();
    let mut schedule = Schedule::default();
    let now = Instant::now();
    assert_eq!(
        schedule.select(vec![1], &[], now),
        [] as [std::vec::Vec<u8>; 0]
    );
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[..256]);
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[256..512]);
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[512..]);
    assert_eq!(schedule.cursors[&vec![1]].0, 168);
    assert_eq!(
        schedule.select(
            vec![2],
            &hashes[..256],
            (now + INTERVAL)
                .checked_sub(Duration::from_nanos(1))
                .unwrap()
        ),
        [] as [std::vec::Vec<u8>; 0]
    );
    schedule.finish(&hashes[0], false);
    assert_eq!(schedule.result(&hashes[0], now), Some(false));
    assert_eq!(schedule.result(&hashes[1], now), Some(true));
    assert_eq!(schedule.result(&hashes[0], now + INTERVAL), None);
    assert_eq!(
        schedule.select(vec![1], &hashes, now + INTERVAL),
        hashes[..256]
    );
    assert_eq!(schedule.result(&[9], now), None);
}

#[test]
fn owner_cursors_stop_at_the_cap_but_tracked_owners_keep_their_window() {
    let mut schedule = Schedule::default();
    let now = Instant::now();
    for owner in 0..u32::try_from(MAX_TRACKED).unwrap() {
        schedule.select(owner.to_le_bytes().to_vec(), &[vec![0]], now);
    }
    assert_eq!(schedule.cursors.len(), MAX_TRACKED);
    let late = u32::try_from(MAX_TRACKED).unwrap().to_le_bytes().to_vec();
    assert_eq!(
        schedule.select(late.clone(), &[vec![1]], now),
        [] as [std::vec::Vec<u8>; 0]
    );
    assert!(!schedule.cursors.contains_key(&late));
    assert_eq!(
        schedule.select(0u32.to_le_bytes().to_vec(), &[vec![1]], now),
        [vec![1]]
    );
}

#[test]
fn recent_hashes_stop_at_the_cap() {
    let hashes = (0..=u32::try_from(MAX_TRACKED).unwrap())
        .map(|i| i.to_le_bytes().to_vec())
        .collect::<Vec<_>>();
    let mut schedule = Schedule::default();
    let now = Instant::now();
    for _ in 0..MAX_TRACKED / WINDOW {
        assert_eq!(schedule.select(vec![1], &hashes, now).len(), WINDOW);
    }
    assert_eq!(schedule.recent.len(), MAX_TRACKED);
    assert_eq!(
        schedule.select(vec![1], &hashes, now),
        [] as [std::vec::Vec<u8>; 0]
    );
}
