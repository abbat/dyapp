//! Combine independent owner queues after a delivery pass.
use super::SearchOutboxSchedule;

/// Next background action after visiting owner queues; the OS controls actual wake timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutboxWake {
    /// No authorized queued work remains.
    #[default]
    Idle,
    /// A due queue remains or a pass failed; use platform retry backoff, never an immediate loop.
    Retry,
    /// No due queue remains; wake at the earliest future queue deadline.
    At {
        /// Unix seconds.
        unix_seconds: u64,
    },
}
impl SearchOutboxWake {
    /// Relative OS delay from the current Unix millisecond clock, saturating at the ABI limit.
    #[must_use]
    pub fn delay_millis(self, now_millis: u64) -> Option<u64> {
        let Self::At { unix_seconds } = self else {
            return None;
        };
        let left = (u128::from(unix_seconds) * 1000).saturating_sub(u128::from(now_millis));
        Some(u64::try_from(left).unwrap_or(u64::MAX))
    }

    /// Classify a queue after its delivery attempt; review-required work cannot auto-send.
    #[must_use]
    pub fn after_attempt(schedule: SearchOutboxSchedule, now: u64) -> Self {
        match schedule {
            SearchOutboxSchedule::At { unix_seconds } if unix_seconds > now => {
                Self::At { unix_seconds }
            }
            SearchOutboxSchedule::At { .. } => Self::Retry,
            SearchOutboxSchedule::Idle | SearchOutboxSchedule::ReviewRequired => Self::Idle,
        }
    }

    /// Combine owners without stranding a failed due queue behind another owner's later deadline.
    /// Retry uses OS backoff; the next pass recalculates all deadlines from persisted state.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Retry, _) | (_, Self::Retry) => Self::Retry,
            (Self::At { unix_seconds: a }, Self::At { unix_seconds: b }) => Self::At {
                unix_seconds: a.min(b),
            },
            (Self::Idle, next) | (next, Self::Idle) => next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchOutboxSchedule, SearchOutboxWake};

    #[test]
    fn blocked_or_empty_queues_do_not_wake_and_due_work_uses_backoff() {
        assert_eq!(
            SearchOutboxWake::after_attempt(SearchOutboxSchedule::Idle, 1000),
            SearchOutboxWake::Idle
        );
        assert_eq!(
            SearchOutboxWake::after_attempt(SearchOutboxSchedule::ReviewRequired, 1000),
            SearchOutboxWake::Idle
        );
        assert_eq!(
            SearchOutboxWake::after_attempt(SearchOutboxSchedule::At { unix_seconds: 1000 }, 1000),
            SearchOutboxWake::Retry
        );
        assert_eq!(
            SearchOutboxWake::after_attempt(SearchOutboxSchedule::At { unix_seconds: 1001 }, 1000),
            SearchOutboxWake::At { unix_seconds: 1001 }
        );
    }

    #[test]
    fn relative_delay_uses_current_subsecond_time_and_saturates() {
        let wake = SearchOutboxWake::At { unix_seconds: 1001 };
        assert_eq!(wake.delay_millis(1_000_123), Some(877));
        assert_eq!(wake.delay_millis(1_001_000), Some(0));
        assert_eq!(wake.delay_millis(1_002_000), Some(0));
        assert_eq!(
            SearchOutboxWake::At {
                unix_seconds: u64::MAX
            }
            .delay_millis(1),
            Some(u64::MAX)
        );
        assert_eq!(SearchOutboxWake::Idle.delay_millis(1), None);
        assert_eq!(SearchOutboxWake::Retry.delay_millis(1), None);
    }

    #[test]
    fn earliest_future_deadline_is_order_independent_and_retry_cannot_be_stranded() {
        let near = SearchOutboxWake::At { unix_seconds: 1001 };
        let far = SearchOutboxWake::At {
            unix_seconds: u64::MAX,
        };
        for (a, b) in [(near, far), (far, near)] {
            assert_eq!(a.merge(b), near);
            assert_eq!(a.merge(SearchOutboxWake::Idle), a);
            assert_eq!(a.merge(SearchOutboxWake::Retry), SearchOutboxWake::Retry);
            assert_eq!(SearchOutboxWake::Retry.merge(a), SearchOutboxWake::Retry);
        }
    }
}
