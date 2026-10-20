//! Profile and heartbeat proof-of-work checks for the search role: Argon2 results by
//! (key, nonce, day), shared with the blocking pool that computes them.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Cache entries before the cache is cleared.
const MAX_ENTRIES: usize = 65_536;

/// A claimed proof of work, checked off the swarm loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowCheck {
    /// The signer's public key.
    pub key: [u8; 32],
    /// The proof-of-work nonce.
    pub nonce: u64,
    /// The claimed leading zero bits.
    pub bits: u32,
    /// The UTC day of a heartbeat proof; `None` for a profile proof.
    pub day: Option<u32>,
}

/// A proof's key, nonce and heartbeat day.
type ProofId = ([u8; 32], u64, Option<u32>);

/// Checked proofs and counts of checks and forged claims.
#[derive(Default)]
pub struct PowCache {
    known: Mutex<HashMap<ProofId, u32>>,
    /// Checks since start; never reset, so metrics and the status line read it independently.
    checks: AtomicU64,
    /// Forged claims since start.
    forged: AtomicU64,
    /// `checks` and `forged` at the last [`PowCache::take_counts`].
    taken: [AtomicU64; 2],
}

impl PowCache {
    /// Whether a cached proof covers the claim; a miss needs [`PowCache::check`].
    pub fn confirmed(&self, c: &PowCheck) -> bool {
        self.known.lock().is_ok_and(|known| {
            known
                .get(&(c.key, c.nonce, c.day))
                .is_some_and(|b| *b >= c.bits)
        })
    }

    /// Runs Argon2 unless cached; caches only a confirmed proof. Blocking: ~15 ms, 8 MiB.
    pub fn check(&self, c: &PowCheck) -> bool {
        if self.confirmed(c) {
            return true;
        }
        self.checks.fetch_add(1, Ordering::Relaxed);
        let actual = match c.day {
            None => dyapp_profile::pow_leading_bits(&c.key, c.nonce),
            Some(day) => dyapp_profile::heartbeat_pow_bits(&c.key, day, c.nonce),
        };
        if actual < c.bits {
            self.forged.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        if let Ok(mut known) = self.known.lock() {
            // ponytail: cleared whole when full; an LRU if profiling shows churn.
            if known.len() >= MAX_ENTRIES {
                known.clear();
            }
            known.insert((c.key, c.nonce, c.day), actual);
        }
        true
    }

    /// Checks and forgeries since start.
    pub fn totals(&self) -> (u64, u64) {
        (
            self.checks.load(Ordering::Relaxed),
            self.forged.load(Ordering::Relaxed),
        )
    }

    /// Checks and forgeries since the last call; the totals go on.
    pub fn take_counts(&self) -> (u64, u64) {
        let (checks, forged) = self.totals();
        let since = |taken: &AtomicU64, total: u64| {
            total.saturating_sub(taken.fetch_max(total, Ordering::Relaxed))
        };
        (since(&self.taken[0], checks), since(&self.taken[1], forged))
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
            day: None,
        };
        assert!(!cache.confirmed(&good));
        assert!(cache.check(&good));
        assert!(cache.check(&good));
        assert_eq!(cache.take_counts(), (1, 0));
        assert!(cache.confirmed(&good));

        let actual = dyapp_profile::pow_leading_bits(&key, nonce + 1);
        let forged = PowCheck {
            nonce: nonce + 1,
            bits: actual + 1,
            ..good.clone()
        };
        assert!(!cache.check(&forged));
        assert!(!cache.confirmed(&PowCheck { bits: 0, ..forged }));
        assert_eq!(cache.take_counts(), (1, 1));
        // Taking the hourly counts leaves the totals that metrics export.
        assert_eq!(cache.totals(), (2, 1));

        // A heartbeat proof is cached per day, apart from the profile proof.
        let day = 20_000;
        let beat = PowCheck {
            nonce: dyapp_profile::solve_heartbeat(&key, day, 2, 0, 1000).unwrap(),
            day: Some(day),
            ..good.clone()
        };
        assert!(cache.check(&beat));
        assert!(cache.confirmed(&beat));
        assert!(!cache.confirmed(&PowCheck {
            day: Some(day + 1),
            ..beat
        }));
    }
}
