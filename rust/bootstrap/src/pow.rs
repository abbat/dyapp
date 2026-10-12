//! Profile proof-of-work checks for the search role: Argon2 results by (key, nonce), shared
//! with the blocking pool that computes them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Cache entries before the cache is cleared.
const MAX_ENTRIES: usize = 65_536;

/// A profile's claimed proof of work, checked off the swarm loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowCheck {
    /// The signer's public key.
    pub key: [u8; 32],
    pub nonce: u64,
    /// The claimed leading zero bits.
    pub bits: u32,
}

#[derive(Default)]
pub struct PowCache {
    known: Mutex<HashMap<([u8; 32], u64), u32>>,
    checks: AtomicU64,
    forged: AtomicU64,
}

impl PowCache {
    /// Whether a cached proof covers `bits`; a miss needs [`PowCache::check`].
    pub fn confirmed(&self, key: &[u8; 32], nonce: u64, bits: u32) -> bool {
        self.known
            .lock()
            .is_ok_and(|known| known.get(&(*key, nonce)).is_some_and(|b| *b >= bits))
    }

    /// Runs Argon2 unless cached; caches only a confirmed proof. Blocking: ~15 ms, 8 MiB.
    pub fn check(&self, c: &PowCheck) -> bool {
        if self.confirmed(&c.key, c.nonce, c.bits) {
            return true;
        }
        self.checks.fetch_add(1, Ordering::Relaxed);
        let actual = dyapp_profile::pow_leading_bits(&c.key, c.nonce);
        if actual < c.bits {
            self.forged.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if let Ok(mut known) = self.known.lock() {
            // ponytail: cleared whole when full; an LRU if profiling shows churn.
            if known.len() >= MAX_ENTRIES {
                known.clear();
            }
            known.insert((c.key, c.nonce), actual);
        }
        true
    }

    /// Checks and forgeries since the last call.
    pub fn take_counts(&self) -> (u64, u64) {
        (
            self.checks.swap(0, Ordering::Relaxed),
            self.forged.swap(0, Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_confirmed_proofs_only() {
        let cache = PowCache::default();
        let key = [5; 32];
        let nonce = dyapp_profile::solve(&key, 2, 0, 1000).unwrap();
        let good = PowCheck {
            key,
            nonce,
            bits: 2,
        };
        assert!(!cache.confirmed(&key, nonce, 2));
        assert!(cache.check(&good));
        assert!(cache.check(&good));
        assert_eq!(cache.take_counts(), (1, 0));
        assert!(cache.confirmed(&key, nonce, 2));

        let actual = dyapp_profile::pow_leading_bits(&key, nonce + 1);
        let forged = PowCheck {
            key,
            nonce: nonce + 1,
            bits: actual + 1,
        };
        assert!(!cache.check(&forged));
        assert!(!cache.confirmed(&key, nonce + 1, 0));
        assert_eq!(cache.take_counts(), (1, 1));
    }
}
