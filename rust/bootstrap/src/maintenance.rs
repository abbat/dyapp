//! Shared write budget and progress for background store rebuilds.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

pub struct RebuildBudget {
    bytes_per_second: u64,
    state: Mutex<State>,
}

struct State {
    next: Instant,
    progress: BTreeMap<String, u8>,
}

impl RebuildBudget {
    /// `mb_per_second` is the positive startup-validated maintenance setting (MiB/s).
    pub fn new(mb_per_second: u32) -> Self {
        assert!(mb_per_second > 0);
        Self {
            bytes_per_second: u64::from(mb_per_second) * (1 << 20),
            state: Mutex::new(State {
                next: Instant::now(),
                progress: BTreeMap::new(),
            }),
        }
    }

    fn reserve(&self, now: Instant, bytes: u64, file: &str, percent: u8) -> Instant {
        let rate = self.bytes_per_second;
        // Round up to avoid granting extra bandwidth to small batches.
        let nanos = (u128::from(bytes % rate) * 1_000_000_000).div_ceil(u128::from(rate));
        let duration = Duration::from_secs(bytes / rate) + Duration::from_nanos(nanos as u64);
        let mut state = self.state.lock().unwrap();
        state.next = state.next.max(now) + duration;
        state.progress.insert(file.into(), percent.min(100));
        state.next
    }

    /// Call after each written batch, outside the store lock and off the swarm loop.
    /// All rebuilds share this budget; simultaneous writers reserve successive time slots.
    pub async fn after_batch(&self, bytes: u64, file: &str, percent: u8) {
        let deadline = self.reserve(Instant::now(), bytes, file, percent);
        tokio::time::sleep_until(deadline).await;
    }

    /// Reports progress on the node's maintenance schedule, including a completed build once.
    pub fn report(&self) {
        let mut state = self.state.lock().unwrap();
        for (file, percent) in &state.progress {
            tracing::info!(file, percent, "store rebuild progress");
        }
        state.progress.retain(|_, percent| *percent < 100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_and_concurrent_writers_stay_within_the_shared_budget() {
        let budget = RebuildBudget::new(1);
        let start = Instant::now();
        let mut now = start;
        let mut written = 0u64;
        for bytes in [1, 4096, 1 << 20, 7, 3 << 20] {
            // Simulate time spent writing each batch before the throttle.
            now += Duration::from_millis(2);
            written += bytes;
            now = budget.reserve(now, bytes, "profiles-idx.db", 50);
            assert!(
                now.duration_since(start).as_nanos() * (1u128 << 20)
                    >= u128::from(written) * 1_000_000_000
            );
        }
        // Two rebuilds finishing their batches together cannot spend the budget twice.
        let first = budget.reserve(now, 1 << 20, "profiles-idx.db", 80);
        let second = budget.reserve(now, 1 << 20, "messages.db", 20);
        assert_eq!(first.duration_since(now), Duration::from_secs(1));
        assert_eq!(second.duration_since(now), Duration::from_secs(2));
    }

    #[tokio::test]
    async fn zero_batch_and_completed_progress() {
        let budget = RebuildBudget::new(16);
        budget.after_batch(0, "profiles-idx.db", 100).await;
        budget.reserve(Instant::now(), 0, "messages.db", 30);
        budget.report();
        let state = budget.state.lock().unwrap();
        assert_eq!(state.progress.len(), 1);
        assert_eq!(state.progress.get("messages.db"), Some(&30));
    }
}
