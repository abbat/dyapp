use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Low bits of a packed timestamp hold the logical counter.
const COUNTER_BITS: u32 = 16;

/// A remote timestamp further ahead of the local wall clock than this is rejected.
pub const MAX_CLOCK_SKEW_MS: u64 = 60_000;

/// Hybrid logical clock timestamp: wall-clock milliseconds in the high 48 bits and a
/// counter in the low 16, so comparing the packed `u64` orders by wall time, then counter.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn new(wall_ms: u64, counter: u16) -> Self {
        Self((wall_ms << COUNTER_BITS) | u64::from(counter))
    }

    pub fn wall_ms(self) -> u64 {
        self.0 >> COUNTER_BITS
    }

    pub fn counter(self) -> u16 {
        (self.0 & 0xffff) as u16
    }

    /// Next timestamp after `self` at wall time `wall_ms`. A full counter carries into
    /// the wall part, which keeps the clock monotonic.
    fn next(self, wall_ms: u64) -> Self {
        if wall_ms > self.wall_ms() {
            Self::new(wall_ms, 0)
        } else {
            Self(self.0 + 1)
        }
    }
}

impl From<u64> for Timestamp {
    fn from(packed: u64) -> Self {
        Self(packed)
    }
}

impl From<Timestamp> for u64 {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

/// A remote timestamp too far ahead of the local wall clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSkew {
    pub remote_ms: u64,
    pub local_ms: u64,
}

impl std::fmt::Display for ClockSkew {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "remote clock {} ms ahead",
            self.remote_ms - self.local_ms
        )
    }
}

impl std::error::Error for ClockSkew {}

/// Hybrid logical clock; clones share state.
#[derive(Clone, Default)]
pub struct HybridClock {
    last: Arc<Mutex<Timestamp>>,
}

impl HybridClock {
    pub fn new() -> Self {
        Self::default()
    }

    /// Timestamp for a local event (sending a message).
    pub fn now(&self) -> Timestamp {
        self.now_at(wall_clock_ms())
    }

    /// Advance past a received timestamp; rejects one more than `MAX_CLOCK_SKEW_MS` ahead.
    pub fn observe(&self, remote: Timestamp) -> Result<Timestamp, ClockSkew> {
        self.observe_at(remote, wall_clock_ms())
    }

    fn now_at(&self, wall_ms: u64) -> Timestamp {
        let mut last = self.last.lock().unwrap();
        *last = last.next(wall_ms);
        *last
    }

    fn observe_at(&self, remote: Timestamp, wall_ms: u64) -> Result<Timestamp, ClockSkew> {
        if remote.wall_ms() > wall_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
            return Err(ClockSkew {
                remote_ms: remote.wall_ms(),
                local_ms: wall_ms,
            });
        }
        let mut last = self.last.lock().unwrap();
        *last = (*last).max(remote).next(wall_ms);
        Ok(*last)
    }
}

fn wall_clock_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Total message order: HLC timestamp, ties broken by sender peer ID.
#[derive(Default)]
pub struct MessageOrdering {
    messages: Vec<(Timestamp, String, String)>,
}

impl MessageOrdering {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, timestamp: Timestamp, sender_id: String, message_id: String) {
        self.messages.push((timestamp, sender_id, message_id));
        self.messages.sort();
    }

    /// Message IDs in order.
    pub fn get_ordered(&self) -> Vec<String> {
        self.messages.iter().map(|m| m.2.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_packs_wall_and_counter() {
        let ts = Timestamp::new(1_700_000_000_000, 7);
        assert_eq!(ts.wall_ms(), 1_700_000_000_000);
        assert_eq!(ts.counter(), 7);
        assert!(Timestamp::new(5, 9) < Timestamp::new(6, 0));
    }

    #[test]
    fn test_now_follows_wall_clock() {
        let clock = HybridClock::new();
        assert_eq!(clock.now_at(100), Timestamp::new(100, 0));
        assert_eq!(clock.now_at(200), Timestamp::new(200, 0));
    }

    #[test]
    fn test_now_counts_when_wall_clock_stalls_or_goes_back() {
        let clock = HybridClock::new();
        clock.now_at(100);
        assert_eq!(clock.now_at(100), Timestamp::new(100, 1));
        assert_eq!(clock.now_at(50), Timestamp::new(100, 2));
    }

    #[test]
    fn test_counter_overflow_stays_monotonic() {
        let clock = HybridClock::new();
        let ts = clock
            .observe_at(Timestamp::new(100, u16::MAX), 100)
            .unwrap();
        assert_eq!(ts, Timestamp::new(101, 0));
        assert_eq!(clock.now_at(100), Timestamp::new(101, 1));
    }

    #[test]
    fn test_observe_moves_past_remote() {
        let clock = HybridClock::new();
        clock.now_at(100);
        let ts = clock.observe_at(Timestamp::new(150, 3), 120).unwrap();
        assert_eq!(ts, Timestamp::new(150, 4));
        assert_eq!(clock.now_at(130), Timestamp::new(150, 5));
    }

    #[test]
    fn test_observe_rejects_clock_too_far_ahead() {
        let clock = HybridClock::new();
        let ahead = Timestamp::new(1_000 + MAX_CLOCK_SKEW_MS + 1, 0);
        assert!(clock.observe_at(ahead, 1_000).is_err());
        assert_eq!(clock.now_at(1_000), Timestamp::new(1_000, 0));
        let edge = Timestamp::new(1_000 + MAX_CLOCK_SKEW_MS, 0);
        assert!(clock.observe_at(edge, 1_000).is_ok());
    }

    #[test]
    fn test_ordering_breaks_ties_by_peer_id() {
        let ts = Timestamp::new(100, 0);
        let mut ordering = MessageOrdering::new();
        ordering.add(Timestamp::new(101, 0), "alice".into(), "late".into());
        ordering.add(ts, "bob".into(), "bob-msg".into());
        ordering.add(ts, "alice".into(), "alice-msg".into());
        assert_eq!(ordering.get_ordered(), vec!["alice-msg", "bob-msg", "late"]);
    }

    #[test]
    fn test_clock_clone_shares_state() {
        let clock = HybridClock::new();
        clock.clone().now_at(100);
        assert_eq!(clock.now_at(100), Timestamp::new(100, 1));
    }
}
