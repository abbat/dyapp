//! Exact scheduling of already signed search publications, without renewing owner intent.
use super::{search_projection::Head, search_publish::State, Error, Store};
use dyapp_identity::SignedRecord;
use rusqlite::{Transaction, TransactionBehavior};

/// The signed search outbox's next action; no signing or network work is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutboxSchedule {
    /// Nothing remains queued, even if an old acknowledged head needs owner review.
    Idle,
    /// An ambiguous legacy withdrawal must be explicitly reauthorized before any request.
    ReviewRequired,
    /// Earliest permitted send time, respecting both the ordinary window and server backoff.
    At {
        /// Unix seconds; a time in the past means the queue is ready now.
        unix_seconds: u64,
    },
}

impl Store {
    fn search_outbox_snapshot(&self) -> Result<(State, SearchOutboxSchedule), Error> {
        let snapshot = Transaction::new_unchecked(&self.db, TransactionBehavior::Deferred)?;
        let (state, head) = self.search_projection()?;
        let schedule = if state.pending.is_none() {
            SearchOutboxSchedule::Idle
        } else if head.is_some_and(Head::needs_withdrawal_recovery) {
            SearchOutboxSchedule::ReviewRequired
        } else {
            SearchOutboxSchedule::At {
                unix_seconds: state.ready_at(),
            }
        };
        snapshot.commit()?;
        Ok((state, schedule))
    }

    /// Inspect the exact next queue deadline in one read snapshot; never migrates or signs.
    ///
    /// # Errors
    /// Returns invalid queue/head state, nested transaction or storage errors.
    pub fn search_outbox_schedule(&self) -> Result<SearchOutboxSchedule, Error> {
        self.search_outbox_snapshot().map(|(_, schedule)| schedule)
    }

    /// Return only an already authorized signed publication once its exact deadline is due.
    ///
    /// # Errors
    /// Returns owner-review-required for a legacy withdrawal, or malformed/storage state.
    pub fn due_search_publication(&self, now: u64) -> Result<Option<SignedRecord>, Error> {
        let (state, schedule) = self.search_outbox_snapshot()?;
        match schedule {
            SearchOutboxSchedule::ReviewRequired => Err(Error::SearchWithdrawalReviewRequired),
            SearchOutboxSchedule::At { unix_seconds } if now >= unix_seconds => Ok(state.pending),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests;
