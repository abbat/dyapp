use super::*;

#[test]
fn cursor_wraps_with_a_256_hash_window_and_rolling_hour() {
    let hashes = (0u32..600)
        .map(|i| i.to_le_bytes().to_vec())
        .collect::<Vec<_>>();
    let mut schedule = Schedule::default();
    let now = Instant::now();
    assert!(schedule.select(vec![1], &[], now).is_empty());
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[..256]);
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[256..512]);
    assert_eq!(schedule.select(vec![1], &hashes, now), hashes[512..]);
    assert_eq!(schedule.cursors[&vec![1]].0, 168);
    assert!(schedule
        .select(
            vec![2],
            &hashes[..256],
            now + INTERVAL - Duration::from_nanos(1)
        )
        .is_empty());
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
