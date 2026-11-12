//! Request, ban, byte-rate and memory limits of the node.

use crate::clock::Clock;
use governor::middleware::NoOpMiddleware;
use governor::nanos::Nanos;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Buckets kept before idle ones are dropped: bounds the map against rotating peer IDs.
const MAX_BUCKETS: usize = 10_000;

type Bucket = RateLimiter<NotKeyed, InMemoryState, Clock, NoOpMiddleware<Nanos>>;

/// Buckets per key with the last time idle ones were dropped.
type Buckets = (HashMap<String, (Bucket, Instant)>, Instant);

/// A request-rate bucket per key (peer ID, IP group or sender key), dropping idle ones once a
/// second and at `MAX_BUCKETS`.
pub struct PeerRateLimiter {
    limiters: Arc<RwLock<Buckets>>,
    messages_per_second: u32,
    clock: Clock,
}

impl PeerRateLimiter {
    /// Allows `messages_per_second` units per key; zero panics on first use.
    #[must_use]
    pub fn new(messages_per_second: u32) -> Self {
        Self::with_clock(messages_per_second, Clock::system())
    }

    /// [`Self::new`] reading time from `clock`.
    #[must_use]
    pub fn with_clock(messages_per_second: u32, clock: Clock) -> Self {
        Self {
            limiters: Arc::new(RwLock::new((HashMap::new(), clock.instant()))),
            messages_per_second,
            clock,
        }
    }

    /// Keep the clock fixed for assertions about a single quota window.
    #[cfg(test)]
    pub(crate) fn frozen(messages_per_second: u32) -> Self {
        Self::with_clock(messages_per_second, crate::clock::Manual::new().clock())
    }

    /// Charges one request to `peer_id`; false when its bucket is empty.
    #[must_use]
    pub fn check_limit(&self, peer_id: &str) -> bool {
        self.check_units(peer_id, 1)
    }

    /// Charges the complete operation atomically; an insufficient bucket spends no units.
    #[must_use]
    ///
    /// # Panics
    ///
    /// Panics if the bucket lock is poisoned or a nonzero charge creates a bucket with a
    /// configured rate of zero.
    pub fn check_units(&self, peer_id: &str, units: u32) -> bool {
        let Some(units) = NonZeroU32::new(units) else {
            return false;
        };
        let mut guard = self.limiters.write().unwrap();
        let (limiters, pruned) = &mut *guard;

        let now = self.clock.instant();
        let second = Duration::from_secs(1);
        // Not only at the cap: one-off keys (fresh peer IDs) would hold the map at its high water.
        if limiters.len() >= MAX_BUCKETS || now - *pruned >= second {
            // A bucket unused for a second has refilled: dropping it changes nothing.
            limiters.retain(|_, (_, seen)| now - *seen < second);
            *pruned = now;
        }
        let (limiter, seen) = limiters.entry(peer_id.to_string()).or_insert_with(|| {
            let quota = Quota::per_second(NonZeroU32::new(self.messages_per_second).unwrap());
            let limiter = RateLimiter::direct_with_clock(quota, self.clock.clone());
            (limiter, now)
        });
        *seen = now;
        matches!(limiter.check_n(units), Ok(Ok(())))
    }

    /// Keys with a bucket.
    #[must_use]
    ///
    /// # Panics
    ///
    /// Panics if the bucket lock is poisoned.
    pub fn get_active_peers(&self) -> usize {
        self.limiters.read().unwrap().0.len()
    }
}

/// Local misbehaviour score per peer: refused floods and bad signatures. Nothing is shared with
/// other nodes. ponytail: keyed by peer ID only, which a client rotates freely; the IP-group
/// limiter bounds what rotation buys.
pub struct Reputation {
    strikes: u32,
    ban: Duration,
    peers: Mutex<HashMap<String, (u32, Instant)>>,
    clock: Clock,
}

impl Reputation {
    /// A peer with `strikes` strikes, each less than `ban` after the previous one, is banned
    /// until `ban` passes without a strike, by `clock`.
    #[must_use]
    pub fn with_clock(strikes: u32, ban: Duration, clock: Clock) -> Self {
        Self {
            strikes,
            ban,
            peers: Mutex::new(HashMap::new()),
            clock,
        }
    }

    /// Counts a strike; returns true when it bans the peer.
    ///
    /// # Panics
    ///
    /// Panics if the reputation-state lock is poisoned.
    pub fn strike(&self, peer: &str) -> bool {
        let now = self.clock.instant();
        let mut peers = self.peers.lock().unwrap();
        if peers.len() >= MAX_BUCKETS {
            peers.retain(|_, (_, last)| now - *last < self.ban);
        }
        let (count, last) = peers.entry(peer.to_string()).or_insert((0, now));
        if now - *last >= self.ban {
            *count = 0;
        }
        (*count, *last) = (count.saturating_add(1), now);
        *count == self.strikes
    }

    /// Whether `peer` is banned now.
    ///
    /// # Panics
    ///
    /// Panics if the reputation-state lock is poisoned.
    pub fn banned(&self, peer: &str) -> bool {
        let now = self.clock.instant();
        let peers = self.peers.lock().unwrap();
        peers
            .get(peer)
            .is_some_and(|(count, last)| *count >= self.strikes && now - *last < self.ban)
    }
}

/// Node protocol bytes in and out per second, against `rate` (0 = no limit).
pub struct Traffic {
    rate: u64,
    /// Bytes counted in the current second (Unix time).
    second: Mutex<(u64, u64)>,
    clock: Clock,
}

impl Traffic {
    /// A counter allowing `rate` bytes per second; 0 = no limit.
    #[must_use]
    pub fn new(rate: u64) -> Self {
        Self::with_clock(rate, Clock::system())
    }

    /// [`Self::new`] reading time from `clock`.
    #[must_use]
    pub fn with_clock(rate: u64, clock: Clock) -> Self {
        Self {
            rate,
            second: Mutex::default(),
            clock,
        }
    }

    /// Counts `bytes` in this second.
    pub fn add(&self, bytes: u64) {
        self.count(bytes);
    }

    /// The share of the byte rate used this second in percent, 0 without a rate.
    // ponytail: fixed one-second window, a burst can straddle two; a token bucket if that matters.
    pub fn second(&self) -> u64 {
        self.count(0)
            .saturating_mul(100)
            .checked_div(self.rate)
            .unwrap_or(0)
    }

    /// Last charged window without advancing the wall-clock window during a test assertion.
    #[cfg(test)]
    pub(crate) fn charged_bytes(&self) -> u64 {
        self.second.lock().unwrap().1
    }

    /// Reserves outgoing bytes only if they fit under `share` percent of the byte rate.
    pub fn try_add(&self, bytes: u64, share: u64) -> bool {
        self.update(bytes, Some(share)).is_some()
    }

    /// Adds `bytes` to this second's count and returns it.
    fn count(&self, bytes: u64) -> u64 {
        self.update(bytes, None).unwrap()
    }

    fn update(&self, bytes: u64, share: Option<u64>) -> Option<u64> {
        let now = self.clock.unix_secs();
        let mut second = self.second.lock().unwrap();
        if second.0 != now {
            *second = (now, 0);
        }
        let total = second.1.saturating_add(bytes);
        if self.rate != 0
            && share.is_some_and(|share| total > self.rate.saturating_mul(share) / 100)
        {
            return None;
        }
        second.1 = total;
        Some(total)
    }
}

/// Test helper: waits until at least half of the current second remains, so the counts a test
/// reads from [`Traffic::second`] do not roll over to the next second halfway.
#[cfg(test)]
pub(crate) fn fresh_second() {
    let into = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_millis());
    if into > 500 {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(1000 - into)));
    }
}

/// The process's physical memory as a share of `max` bytes (0 = no limit), measured like
/// libp2p's memory connection limit and sampled at most every 100 ms.
pub struct Memory {
    max: u64,
    sample: Mutex<Option<(Instant, u64)>>,
    clock: Clock,
}

impl Memory {
    /// A sampler against `max` bytes; 0 = no limit.
    #[must_use]
    pub fn new(max: u64) -> Self {
        Self {
            max,
            sample: Mutex::default(),
            clock: Clock::system(),
        }
    }

    /// The share of `max` in percent, 0 without a limit or a measurement.
    ///
    /// # Panics
    ///
    /// Panics if memory limiting is enabled and the sample-state lock is poisoned.
    pub fn share(&self) -> u64 {
        if self.max == 0 {
            return 0;
        }
        let now = self.clock.instant();
        let mut sample = self.sample.lock().unwrap();
        match *sample {
            Some((at, share)) if now < at + Duration::from_millis(100) => share,
            _ => {
                let used = memory_stats::memory_stats().map_or(0, |m| m.physical_mem as u64);
                let share = used.saturating_mul(100) / self.max;
                *sample = Some((now, share));
                share
            }
        }
    }

    /// Fixes the share for an hour.
    #[cfg(test)]
    ///
    /// # Panics
    ///
    /// Panics if the sample-state lock is poisoned.
    pub fn set(&self, share: u64) {
        let later = self.clock.instant() + Duration::from_secs(3600);
        *self.sample.lock().unwrap() = Some((later, share));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Manual;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn memory_share_of_the_limit() {
        assert_eq!(Memory::new(0).share(), 0);
        assert!(Memory::new(1).share() > 100, "no measurement");
        assert_eq!(Memory::new(u64::MAX).share(), 0);
    }

    #[test]
    fn traffic_counts_the_byte_rate() {
        let clock = Manual::new();
        let rate = Traffic::with_clock(1000, clock.clock());
        rate.add(800);
        assert_eq!(rate.second(), 80);
        let repair = Traffic::with_clock(1000, clock.clock());
        assert!(repair.try_add(700, 75));
        assert!(!repair.try_add(51, 75));
        assert!(repair.try_add(50, 75), "failed reservation spent bytes");
        assert_eq!(repair.second(), 75);
        assert!(!repair.try_add(1, 75));
        let unlimited = Traffic::new(0);
        assert!(unlimited.try_add(u64::MAX, 75));
        unlimited.add(u64::MAX);
        assert_eq!(unlimited.second(), 0);
    }

    #[test]
    fn traffic_window_is_the_wall_second() {
        let clock = Manual::at(100_000);
        let traffic = Traffic::with_clock(1000, clock.clock());
        traffic.add(400);
        clock.advance(999 * MS);
        assert_eq!(traffic.second(), 40, "reset within the second");
        clock.advance(MS);
        assert_eq!(traffic.second(), 0, "kept past the second");
        traffic.add(300);
        clock.set_unix(100_000 - 3 * 3600);
        assert_eq!(traffic.second(), 0, "kept after the clock went back");
        traffic.add(200);
        clock.set_unix(100_000 + 3 * 3600);
        assert_eq!(traffic.second(), 0, "kept after the clock jumped forward");
    }

    #[test]
    fn strikes_ban_until_quiet() {
        let clock = Manual::new();
        let ban = Duration::from_secs(300);
        let almost = Duration::from_millis(299_999);
        let reputation = Reputation::with_clock(2, ban, clock.clock());
        assert!(!reputation.strike("bad"));
        assert!(!reputation.banned("bad"));
        clock.advance(almost);
        assert!(reputation.strike("bad"), "a strike just inside the window");
        assert!(reputation.banned("bad") && !reputation.banned("good"));
        clock.set_unix(0);
        clock.advance(almost);
        assert!(reputation.banned("bad"), "unbanned before the ban passed");
        clock.advance(MS);
        assert!(!reputation.banned("bad"), "banned after the ban passed");
        assert!(!reputation.strike("bad"), "old strikes were kept");
        clock.advance(ban);
        assert!(
            !reputation.strike("bad"),
            "a strike a whole window later counted"
        );
    }

    #[test]
    fn bucket_refills_at_the_quota_rate_on_monotonic_time() {
        let clock = Manual::new();
        let limiter = PeerRateLimiter::with_clock(2, clock.clock());
        assert!(limiter.check_limit("p") && limiter.check_limit("p"));
        assert!(!limiter.check_limit("p"));
        clock.set_unix(0);
        assert!(
            !limiter.check_limit("p"),
            "a wall-clock jump refilled the bucket"
        );
        clock.advance(499 * MS);
        assert!(!limiter.check_limit("p"), "refilled early");
        clock.advance(MS);
        assert!(limiter.check_limit("p"), "not refilled at half a second");
        assert!(!limiter.check_limit("p"));
        clock.advance(Duration::from_secs(3600));
        assert!(limiter.check_units("p", 2), "an idle bucket did not refill");
        assert!(!limiter.check_units("p", 1), "refilled past the burst");
    }

    #[test]
    fn idle_buckets_are_dropped_at_the_cap() {
        let clock = Manual::new();
        let limiter = PeerRateLimiter::with_clock(1, clock.clock());
        assert!(limiter.check_limit("busy"));
        for peer in 1..MAX_BUCKETS {
            assert!(limiter.check_limit(&peer.to_string()));
        }
        assert!(!limiter.check_limit("busy"), "an active bucket was reset");
        clock.advance(Duration::from_secs(1));
        assert!(limiter.check_limit("new"));
        assert_eq!(limiter.get_active_peers(), 1);
    }

    #[test]
    fn idle_buckets_are_dropped_after_a_second_below_the_cap() {
        let clock = Manual::new();
        let limiter = PeerRateLimiter::with_clock(1, clock.clock());
        for peer in 0..3 {
            assert!(limiter.check_limit(&peer.to_string()));
        }
        clock.advance(999 * MS);
        assert!(
            !limiter.check_limit("0"),
            "the bucket was dropped within a second"
        );
        assert_eq!(limiter.get_active_peers(), 3);
        clock.advance(Duration::from_secs(1));
        assert!(limiter.check_limit("new"));
        assert_eq!(limiter.get_active_peers(), 1);
    }

    #[test]
    fn multi_unit_charge_is_atomic_and_requires_capacity() {
        let limiter = PeerRateLimiter::new(5);
        assert!(limiter.check_units("put", 5));
        assert!(!limiter.check_limit("put"));
        assert!(!limiter.check_units("get", 6));
        assert!(limiter.check_units("get", 5), "failed charge spent units");
        assert!(!PeerRateLimiter::new(4).check_units("put", 5));
    }

    #[test]
    fn test_rate_limit_creation() {
        let limiter = PeerRateLimiter::new(10);
        assert_eq!(limiter.messages_per_second, 10);
    }

    #[test]
    fn test_rate_limit_allows_messages() {
        let limiter = PeerRateLimiter::new(100);
        assert!(limiter.check_limit("alice"));
    }

    #[test]
    fn test_different_peers_have_separate_limits() {
        let limiter = PeerRateLimiter::new(1);

        assert!(limiter.check_limit("alice"));
        assert!(!limiter.check_limit("alice")); // Alice hits limit

        assert!(limiter.check_limit("bob")); // Bob has fresh limit
    }

    #[test]
    fn test_active_peers_tracking() {
        let limiter = PeerRateLimiter::new(10);

        assert!(limiter.check_limit("alice"));
        assert!(limiter.check_limit("bob"));
        assert!(limiter.check_limit("charlie"));

        assert_eq!(limiter.get_active_peers(), 3);
    }
}
