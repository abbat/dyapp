use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, RwLock};

pub struct PeerRateLimiter {
    limiters: Arc<RwLock<HashMap<String, DefaultDirectRateLimiter>>>,
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

        let limiter = limiters.entry(peer_id.to_string()).or_insert_with(|| {
            RateLimiter::direct(Quota::per_second(
                NonZeroU32::new(self.messages_per_second).unwrap(),
            ))
        });

        limiter.check().is_ok()
    }

    pub fn cleanup_inactive(&self, _max_age_secs: u64) {
        // In a real implementation, track timestamps and clean up old limiters
        // For now, this is a stub for future enhancement
    }

    pub fn get_active_peers(&self) -> usize {
        self.limiters.read().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
