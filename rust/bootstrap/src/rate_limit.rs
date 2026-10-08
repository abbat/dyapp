use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
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
        limiter.check().is_ok()
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

/// Node protocol bytes in and out this calendar month (UTC), kept in `file` across restarts.
pub struct Traffic {
    cap: u64,
    file: PathBuf,
    used: Mutex<(String, u64)>,
}

impl Traffic {
    /// `cap` in bytes, 0 = no cap. Starts from the count saved in `file` for this month.
    pub fn new(cap: u64, file: PathBuf) -> Self {
        let saved = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| {
                let (month, bytes) = text.trim().split_once(' ')?;
                Some((month.to_string(), bytes.parse().ok()?))
            })
            .unwrap_or_default();
        Self {
            cap,
            file,
            used: Mutex::new(saved),
        }
    }

    /// Counts `bytes`; returns the share of the cap used this month in percent, 0 without a cap.
    pub fn add(&self, bytes: u64) -> u64 {
        let month = chrono::Utc::now().format("%Y-%m").to_string();
        let mut used = self.used.lock().unwrap();
        if used.0 != month {
            *used = (month, 0);
        }
        used.1 = used.1.saturating_add(bytes);
        used.1
            .saturating_mul(100)
            .checked_div(self.cap)
            .unwrap_or(0)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let (month, bytes) = self.used.lock().unwrap().clone();
        std::fs::write(&self.file, format!("{month} {bytes}\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_survives_restart_within_a_month() {
        let dir = format!("/tmp/ai/test-traffic-{}", uuid::Uuid::new_v4());
        std::fs::create_dir_all(&dir).unwrap();
        let file = PathBuf::from(format!("{dir}/traffic"));
        let traffic = Traffic::new(1000, file.clone());
        assert_eq!(traffic.add(500), 50);
        traffic.save().unwrap();
        assert_eq!(Traffic::new(1000, file.clone()).add(400), 90);
        // A count from another month is not carried over.
        std::fs::write(&file, "2000-01 999\n").unwrap();
        assert_eq!(Traffic::new(1000, file).add(100), 10);
        assert_eq!(
            Traffic::new(0, PathBuf::from("/nonexistent")).add(u64::MAX),
            0
        );
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
