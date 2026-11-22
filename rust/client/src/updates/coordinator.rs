//! Installation-level update admission; transport and durable checkpoint writes stay in core integration.
use super::{check_due, CheckState, Interval, Trigger};
use std::sync::{Arc, Weak};

/// A checkpoint to persist before starting IO and after completing it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Checkpoint {
    /// Installation-wide interval, independent of the active profile.
    pub interval: Interval,
    /// Existing wall-clock/backoff policy state.
    pub state: CheckState,
    /// An interrupted process must count an unfinished request as a failure on restore.
    pub in_flight: bool,
}

/// Exact live request capability; dropping it abandons the attempt without stranding admission.
#[derive(Debug)]
#[must_use = "Complete the admitted request, or drop its ticket to abandon it"]
pub struct Attempt(Arc<()>);

/// A validated 200/cached 304, or any failed/canceled transport or validation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Completion {
    /// Transport and feed validation already succeeded in the shared checker.
    Validated,
    /// No new trusted state was obtained.
    Failed,
}

/// Own one coordinator per installation; serialize access, never per identity or screen.
#[derive(Debug, Default)]
pub struct Coordinator {
    interval: Interval,
    state: CheckState,
    pending: Option<Weak<()>>,
}

impl Coordinator {
    /// Restore durable state; an interrupted request consumes one failure, not a new request.
    #[must_use]
    pub fn restore(checkpoint: Checkpoint) -> Self {
        let mut state = checkpoint.state;
        if checkpoint.in_flight {
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        }
        Self {
            interval: checkpoint.interval,
            state,
            pending: None,
        }
    }

    /// Persist this result after admission and completion; it contains no live capability.
    #[must_use]
    pub fn checkpoint(&mut self) -> Checkpoint {
        self.reap();
        Checkpoint {
            interval: self.interval,
            state: self.state,
            in_flight: self.pending.is_some(),
        }
    }

    /// Changing interval does not cancel or duplicate an already admitted request.
    pub fn set_interval(&mut self, interval: Interval) {
        self.interval = interval;
    }

    /// Coalesce every trigger while a ticket lives; otherwise apply the shared cadence rule.
    pub fn begin(&mut self, now: u64, jitter: u64, trigger: Trigger) -> Option<Attempt> {
        self.reap();
        if self.pending.is_some() || !check_due(now, self.interval, self.state, jitter, trigger) {
            return None;
        }
        // Admit rollback catch-up once, then retain ordinary failure backoff on later triggers.
        self.clamp(now);
        self.state.last_attempt = Some(now);
        let token = Arc::new(());
        self.pending = Some(Arc::downgrade(&token));
        Some(Attempt(token))
    }

    /// Only the exact live ticket can update state; a rejected foreign ticket is consumed too.
    pub fn finish(&mut self, attempt: Attempt, now: u64, completion: Completion) -> bool {
        let Some(current) = self.pending.as_ref().and_then(Weak::upgrade) else {
            self.reap();
            return false;
        };
        if !Arc::ptr_eq(&current, &attempt.0) {
            return false;
        }
        drop(attempt);
        self.pending = None;
        self.clamp(now);
        match completion {
            Completion::Validated => {
                self.state.last_success = Some(now);
                self.state.consecutive_failures = 0;
            }
            Completion::Failed => self.failed(),
        }
        true
    }

    fn clamp(&mut self, now: u64) {
        self.state.last_attempt = self.state.last_attempt.map(|at| at.min(now));
        self.state.last_success = self.state.last_success.map(|at| at.min(now));
    }

    fn failed(&mut self) {
        self.state.consecutive_failures = self.state.consecutive_failures.saturating_add(1);
    }

    fn reap(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|token| token.upgrade().is_none())
        {
            self.pending = None;
            self.failed();
        }
    }
}

#[cfg(test)]
mod tests;
