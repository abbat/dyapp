use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Buckets kept before idle ones are dropped: bounds the map against rotating peer IDs.
const MAX_BUCKETS: usize = 10_000;

pub struct PeerRateLimiter {
    limiters: Arc<RwLock<HashMap<String, (DefaultDirectRateLimiter, Instant)>>>,
    messages_per_second: u32,
}

impl PeerRateLimiter {
    pub fn new(messages_per_second: u32) -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
            messages_per_second,
        }
    }

    pub fn check_limit(&self, peer_id: &str) -> bool {
        self.check_units(peer_id, 1)
    }

    /// Charges the complete operation atomically; an insufficient bucket spends no units.
    pub fn check_units(&self, peer_id: &str, units: u32) -> bool {
        let Some(units) = NonZeroU32::new(units) else {
            return false;
        };
        let mut limiters = self.limiters.write().unwrap();

        let now = Instant::now();
        if limiters.len() >= MAX_BUCKETS {
            // A bucket unused for a second has refilled: dropping it changes nothing.
            limiters.retain(|_, (_, seen)| now - *seen < Duration::from_secs(1));
        }
        let (limiter, seen) = limiters.entry(peer_id.to_string()).or_insert_with(|| {
            let quota = Quota::per_second(NonZeroU32::new(self.messages_per_second).unwrap());
            (RateLimiter::direct(quota), now)
        });
        *seen = now;
        matches!(limiter.check_n(units), Ok(Ok(())))
    }

    pub fn get_active_peers(&self) -> usize {
        self.limiters.read().unwrap().len()
    }
}

/// Local misbehaviour score per peer: refused floods and bad signatures. Nothing is shared with
/// other nodes. ponytail: keyed by peer ID only, which a client rotates freely; the IP-group
/// limiter bounds what rotation buys.
pub struct Reputation {
    strikes: u32,
    ban: Duration,
    peers: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Reputation {
    /// A peer with `strikes` strikes, each less than `ban` after the previous one, is banned
    /// until `ban` passes without a strike.
    pub fn new(strikes: u32, ban: Duration) -> Self {
        Self {
            strikes,
            ban,
            peers: Mutex::new(HashMap::new()),
        }
    }

    /// Counts a strike; returns true when it bans the peer.
    pub fn strike(&self, peer: &str) -> bool {
        let now = Instant::now();
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

    pub fn banned(&self, peer: &str) -> bool {
        let peers = self.peers.lock().unwrap();
        peers
            .get(peer)
            .is_some_and(|(count, last)| *count >= self.strikes && last.elapsed() < self.ban)
    }
}

/// Node protocol bytes in and out per second, against `rate` (0 = no limit).
pub struct Traffic {
    rate: u64,
    /// Bytes counted in the current second (Unix time).
    second: Mutex<(u64, u64)>,
}

impl Traffic {
    pub fn new(rate: u64) -> Self {
        Self {
            rate,
            second: Mutex::default(),
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

    /// Adds `bytes` to this second's count and returns it.
    fn count(&self, bytes: u64) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let mut second = self.second.lock().unwrap();
        if second.0 != now {
            *second = (now, 0);
        }
        second.1 = second.1.saturating_add(bytes);
        second.1
    }
}

/// The process's physical memory as a share of `max` bytes (0 = no limit), measured like
/// libp2p's memory connection limit and sampled at most every 100 ms.
pub struct Memory {
    max: u64,
    sample: Mutex<Option<(Instant, u64)>>,
}

impl Memory {
    pub fn new(max: u64) -> Self {
        Self {
            max,
            sample: Mutex::default(),
        }
    }

    /// The share of `max` in percent, 0 without a limit or a measurement.
    pub fn share(&self) -> u64 {
        if self.max == 0 {
            return 0;
        }
        let now = Instant::now();
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
    pub fn set(&self, share: u64) {
        let later = Instant::now() + Duration::from_secs(3600);
        *self.sample.lock().unwrap() = Some((later, share));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_share_of_the_limit() {
        assert_eq!(Memory::new(0).share(), 0);
        assert!(Memory::new(1).share() > 100, "no measurement");
        assert_eq!(Memory::new(u64::MAX).share(), 0);
    }

    #[test]
    fn traffic_counts_the_byte_rate() {
        // ponytail: a second boundary between the calls resets the count; rare enough.
        let rate = Traffic::new(1000);
        rate.add(800);
        assert_eq!(rate.second(), 80);
        let unlimited = Traffic::new(0);
        unlimited.add(u64::MAX);
        assert_eq!(unlimited.second(), 0);
    }

    #[test]
    fn strikes_ban_until_quiet() {
        let reputation = Reputation::new(2, Duration::from_millis(300));
        assert!(!reputation.strike("bad"));
        assert!(!reputation.banned("bad"));
        assert!(reputation.strike("bad"));
        assert!(reputation.banned("bad") && !reputation.banned("good"));
        std::thread::sleep(Duration::from_millis(350));
        assert!(!reputation.banned("bad"));
        assert!(!reputation.strike("bad"), "old strikes were kept");
    }

    #[test]
    fn idle_buckets_are_dropped_at_the_cap() {
        let limiter = PeerRateLimiter::new(1);
        assert!(limiter.check_limit("busy"));
        for peer in 1..MAX_BUCKETS {
            limiter.check_limit(&peer.to_string());
        }
        assert!(!limiter.check_limit("busy"), "an active bucket was reset");
        std::thread::sleep(Duration::from_millis(1100));
        limiter.check_limit("new");
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

        limiter.check_limit("alice");
        limiter.check_limit("bob");
        limiter.check_limit("charlie");

        assert_eq!(limiter.get_active_peers(), 3);
    }
}
