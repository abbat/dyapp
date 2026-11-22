//! Durable app-wide update orchestration. Native lifecycle wiring and activation remain separate.
use super::coordinator::{Completion, Coordinator};
use super::snapshot::{self, Snapshot};
use super::storage::{self, LockedState};
use super::transport::{self, Cache, Client};
use super::{Interval, Trigger};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Expected check failure, retaining the prior trusted cache rather than a guessed success.
#[derive(Debug)]
pub enum Failure {
    /// The fixed-origin request failed.
    Transport(transport::Error),
    /// The candidate failed shared rollback admission.
    Rejected(snapshot::Error),
    /// Completion time could not be represented; no candidate was admitted.
    Clock,
}

/// No validated result is exposed until its complete snapshot is durably saved.
#[derive(Debug)]
pub enum Outcome {
    /// Another same-installation operation owns the independent update lease.
    Busy,
    /// Cadence/backoff refused a request; restored interrupted state is already persisted.
    NotDue(Snapshot),
    /// A validated response and completion checkpoint were committed.
    Validated(Snapshot),
    /// The attempt failed; the prior trusted cache and new failure cadence were committed.
    Failed {
        /// Complete durable state, with no guessed replacement feed.
        snapshot: Snapshot,
        /// Typed failure for native presentation mapping.
        reason: Failure,
    },
}

/// Errors prevent a new validated result from being exposed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Admission/completion state could not be durably stored.
    #[error(transparent)]
    Storage(#[from] storage::Error),
    /// The current wall clock precedes the Unix epoch.
    #[error("update wall clock unavailable")]
    Clock,
}

/// Check on an IO worker using only the fixed-origin client and installation-wide state.
/// Invoke only after the live feed and platform delivery gate have been accepted.
///
/// # Errors
/// Returns clock/storage failures before exposing any new validated state.
pub fn check(root: &Path, trigger: Trigger) -> Result<Outcome, Error> {
    run(root, trigger, now, |cached| Client::new().fetch(cached))
}

/// Persist an interval immediately without starting or canceling a request.
///
/// # Errors
/// Returns snapshot/storage failures without erasing the live request or its trusted cache.
pub fn set_interval(root: &Path, interval: Interval) -> Result<Snapshot, Error> {
    Ok(storage::change_interval(root, interval)?)
}

fn now() -> Result<u64, Error> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| Error::Clock)
}

fn run(
    root: &Path,
    trigger: Trigger,
    mut clock: impl FnMut() -> Result<u64, Error>,
    fetch: impl FnOnce(Option<&Cache>) -> Result<Cache, transport::Error>,
) -> Result<Outcome, Error> {
    let Some(mut state) = LockedState::try_open(root)? else {
        return Ok(Outcome::Busy);
    };
    let started = clock()?;
    let (snapshot, (mut coordinator, attempt)) = state.update(|snapshot| {
        let mut coordinator = Coordinator::restore(snapshot.checkpoint());
        let attempt = coordinator.begin(started, snapshot.jitter(), trigger);
        snapshot.set_checkpoint(coordinator.checkpoint());
        Ok((coordinator, attempt))
    })?;
    let Some(attempt) = attempt else {
        return Ok(Outcome::NotDue(snapshot));
    };
    let fetched = fetch(snapshot.cache());
    let (finished, candidate) = match clock() {
        Ok(at) => (at, fetched.map_err(Failure::Transport)),
        Err(_) => (started, Err(Failure::Clock)),
    };
    let (snapshot, failure) = state.update(|snapshot| {
        coordinator.set_interval(snapshot.checkpoint().interval);
        let failure = candidate
            .and_then(|cache| snapshot.admit(cache).map_err(Failure::Rejected))
            .err();
        let completion = if failure.is_none() {
            Completion::Validated
        } else {
            Completion::Failed
        };
        if !coordinator.finish(attempt, finished, completion) {
            return Err(storage::Error::Incompatible);
        }
        snapshot.set_checkpoint(coordinator.checkpoint());
        Ok(failure)
    })?;
    Ok(match failure {
        None => Outcome::Validated(snapshot),
        Some(reason) => Outcome::Failed { snapshot, reason },
    })
}

#[cfg(test)]
mod tests;
