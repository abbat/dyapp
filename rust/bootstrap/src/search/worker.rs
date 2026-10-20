//! The background rebuild of the search index into its target generation.
use super::{Abort, Candidate, Replay, SearchIndex};
use crate::maintenance::RebuildBudget;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Consecutive aborts of one kind before the next attempt is forced.
const FORCE_AFTER: u32 = 3;
/// Wait before replaying a postponed candidate.
const POSTPONE_DELAY: Duration = Duration::from_secs(600);
/// Wait before rebuilding after any other abort.
const RETRY_DELAY: Duration = Duration::from_secs(3600);

/// Rebuild counters since start.
#[derive(Default)]
pub(super) struct Stats {
    postpones: AtomicU64,
    overflows: AtomicU64,
    drop_refills: AtomicU64,
    stop_the_worlds: AtomicU64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Disk,
    Postpone,
    Overflow,
    Io,
}

/// The kind of the last aborts and how many came in a row.
#[derive(Default)]
struct Streak {
    kind: Option<Kind>,
    count: u32,
}

impl Streak {
    fn record(&mut self, kind: Kind) {
        if self.kind == Some(kind) {
            self.count += 1;
        } else {
            *self = Self {
                kind: Some(kind),
                count: 1,
            };
        }
    }

    /// The kind to force: three in a row of anything but a store error.
    fn forced(&self) -> Option<Kind> {
        self.kind
            .filter(|kind| *kind != Kind::Io && self.count >= FORCE_AFTER)
    }
}

/// The rebuild state for the status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// `refilling`, `rebuilding` or `serving`.
    pub phase: &'static str,
    /// Builds postponed because replay did not settle.
    pub postpones: u64,
    /// Builds stopped because more keys changed than tracking holds.
    pub overflows: u64,
    /// Forced drops for a refill by catch-up after three disk aborts in a row.
    pub drop_refills: u64,
    /// Forced activations that replay every changed key under the writer lock.
    pub stop_the_worlds: u64,
}

impl SearchIndex {
    /// Rebuilds into the target generation until it is active, then returns. Runs on a task of
    /// its own; searches and writes go on meanwhile.
    pub async fn rebuild(&self, budget: &RebuildBudget, min_free: u64) {
        self.rebuild_with(budget, min_free, [POSTPONE_DELAY, RETRY_DELAY])
            .await;
    }

    /// `rebuild` with the delays after a postpone and after other aborts.
    pub(super) async fn rebuild_with(
        &self,
        budget: &RebuildBudget,
        min_free: u64,
        delays: [Duration; 2],
    ) {
        let mut streak = Streak::default();
        let mut pending: Option<Candidate> = None;
        while self.index_version() < self.target {
            let result = match streak.forced() {
                Some(Kind::Disk) => {
                    tracing::warn!(step = "drop_refill", "search index rebuild forced");
                    self.rebuild.drop_refills.fetch_add(1, Ordering::Relaxed);
                    self.refill()
                }
                Some(_) => {
                    tracing::warn!(step = "stop_the_world", "search index rebuild forced");
                    self.rebuild.stop_the_worlds.fetch_add(1, Ordering::Relaxed);
                    let built = match pending.take() {
                        Some(candidate) => Ok(candidate),
                        None => self.build(self.target, budget, min_free).await,
                    };
                    match built {
                        Ok(candidate) | Err(Abort::Postpone(candidate)) => {
                            self.activate(candidate, Replay::All)
                        }
                        Err(abort) => Err(abort),
                    }
                }
                None => {
                    let built = match pending.take() {
                        Some(candidate) => self.replay(candidate, budget, min_free).await,
                        None => self.build(self.target, budget, min_free).await,
                    };
                    built.and_then(|candidate| self.activate(candidate, Replay::Settled))
                }
            };
            let (kind, delay) = match result {
                Ok(()) => continue,
                Err(Abort::Postpone(candidate)) => {
                    pending = Some(candidate);
                    self.rebuild.postpones.fetch_add(1, Ordering::Relaxed);
                    (Kind::Postpone, delays[0])
                }
                Err(Abort::Overflow) => {
                    self.rebuild.overflows.fetch_add(1, Ordering::Relaxed);
                    (Kind::Overflow, delays[1])
                }
                Err(Abort::Disk) => (Kind::Disk, delays[1]),
                Err(Abort::Io(error)) => {
                    tracing::error!(%error, "search index rebuild failed");
                    (Kind::Io, delays[1])
                }
            };
            streak.record(kind);
            tracing::info!(
                abort = ?kind,
                in_a_row = streak.count,
                retry_s = delay.as_secs(),
                "search index rebuild stopped"
            );
            tokio::time::sleep(delay).await;
        }
    }

    /// The rebuild phase and counters since start.
    pub fn rebuild_report(&self) -> Report {
        let phase = if self.refilling() {
            "refilling"
        } else if self.index_version() < self.target {
            "rebuilding"
        } else {
            "serving"
        };
        let stats = &self.rebuild;
        Report {
            phase,
            postpones: stats.postpones.load(Ordering::Relaxed),
            overflows: stats.overflows.load(Ordering::Relaxed),
            drop_refills: stats.drop_refills.load(Ordering::Relaxed),
            stop_the_worlds: stats.stop_the_worlds.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streak_forces_after_three_of_a_kind_but_not_store_errors() {
        let mut streak = Streak::default();
        for kind in [Kind::Disk, Kind::Disk, Kind::Overflow, Kind::Overflow] {
            streak.record(kind);
            assert_eq!(streak.forced(), None);
        }
        streak.record(Kind::Overflow);
        assert_eq!(streak.forced(), Some(Kind::Overflow));
        for _ in 0..5 {
            streak.record(Kind::Io);
        }
        assert_eq!(streak.forced(), None);
    }
}
