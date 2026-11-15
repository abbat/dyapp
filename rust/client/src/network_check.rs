//! Saved connection drafts and single-use activation of a current successful seed check.
//!
//! Callers hold their identity operation lease through check/activation. This state never
//! starts or stops an installation node; seed probes use the existing throwaway client key.

use crate::cancellation::Cancellation;
use crate::seeds::{self, Row};
use dyapp_identity::network::{self, Network, Settings};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Why a draft check or activation was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// No seed rows were supplied.
    #[error("empty")]
    Empty,
    /// The network name violates the existing protocol rules.
    #[error("invalidNetwork: {0}")]
    InvalidNetwork(String),
    /// The attempt was cancelled, replaced or invalidated by an identity transition.
    #[error("cancelled")]
    Cancelled,
    /// This directory has no current, unconsumed successful check.
    #[error("notChecked")]
    NotChecked,
    /// Existing storage or network error, retained for the platform's diagnostics.
    #[error("{0}")]
    Failed(String),
}

impl From<seeds::CheckError> for Error {
    fn from(error: seeds::CheckError) -> Self {
        match error {
            seeds::CheckError::NoSeeds => Self::Empty,
            seeds::CheckError::Name(reason) => Self::InvalidNetwork(reason),
            error @ seeds::CheckError::Swarm(_) => Self::Failed(error.to_string()),
        }
    }
}

#[derive(Default)]
struct State {
    current: Option<Arc<Cancellation>>,
    connected: Option<(PathBuf, Network)>,
}

impl State {
    fn clear(&mut self) {
        if let Some(current) = self.current.take() {
            current.cancel();
        }
        self.connected = None;
    }
}

/// One identity-transition-aware checker, shared by desktop and common native adapters.
#[derive(Default)]
pub struct Checks {
    state: Mutex<State>,
}

impl Checks {
    fn state(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.state
            .lock()
            .map_err(|error| Error::Failed(error.to_string()))
    }

    /// Invalidate completed checks and signal registered work. An older completion cannot
    /// restore activation permission, including after this call returns.
    ///
    /// # Errors
    /// Returns a synchronization failure if the state mutex was poisoned.
    pub fn clear_checked(&self) -> Result<(), Error> {
        self.state()?.clear();
        Ok(())
    }

    /// Cancel current work and its activation permission; files are left unchanged.
    pub fn cancel(&self) {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Save the draft, then dial its rows. Starting a check consumes any previous success.
    /// `ready` runs after cancellation registration and reports an already-started identity
    /// transition; it may also signal test readiness without a guessed delay.
    ///
    /// # Errors
    /// Returns typed validation, cancellation or storage/network failures. The saved draft
    /// survives later check failure. Pre-cancelled attempts do not write it.
    pub async fn check(
        &self,
        dir: PathBuf,
        draft: Network,
        wait: Duration,
        cancellation: &Cancellation,
        ready: impl FnOnce() -> Result<bool, Error>,
    ) -> Result<Vec<Row>, Error> {
        if cancellation.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let attempt = self.begin(&dir, &draft)?;
        if ready()? {
            attempt.cancel();
        }
        let rows = tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(Error::Cancelled),
            () = attempt.cancelled() => return Err(Error::Cancelled),
            rows = seeds::check(&draft, wait) => rows,
        };
        self.complete(&attempt, dir, draft, rows)
    }

    fn begin(&self, dir: &Path, draft: &Network) -> Result<Arc<Cancellation>, Error> {
        let mut state = self.state()?;
        state.clear();
        // Serialize both writes and registration so concurrent checks cannot save out of order.
        network::save(dir, Settings::Draft, draft)
            .map_err(|error| Error::Failed(error.to_string()))?;
        let attempt = Arc::new(Cancellation::new());
        state.current = Some(attempt.clone());
        Ok(attempt)
    }

    fn complete(
        &self,
        attempt: &Arc<Cancellation>,
        dir: PathBuf,
        draft: Network,
        rows: Result<Vec<Row>, seeds::CheckError>,
    ) -> Result<Vec<Row>, Error> {
        let mut state = self.state()?;
        if attempt.is_cancelled()
            || !state
                .current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, attempt))
        {
            return Err(Error::Cancelled);
        }
        let rows = rows.map_err(Error::from)?;
        if rows.iter().any(|row| matches!(row, Row::Connected(_))) {
            state.connected = Some((dir, draft));
        }
        Ok(rows)
    }

    /// Activate once, only for the checked directory. A save failure retains permission for
    /// an explicit retry; a successful save invalidates all earlier completions.
    ///
    /// # Errors
    /// Returns `NotChecked` for absent, consumed or foreign-directory success, or the storage
    /// failure without claiming activation succeeded.
    pub fn activate(&self, dir: &Path) -> Result<Network, Error> {
        self.activate_with(dir, |dir, network| {
            network::save(dir, Settings::Active, network)
        })
    }

    fn activate_with(
        &self,
        dir: &Path,
        save: impl FnOnce(&Path, &Network) -> std::io::Result<()>,
    ) -> Result<Network, Error> {
        let mut state = self.state()?;
        let network = match &state.connected {
            Some((checked_dir, network)) if checked_dir == dir => network.clone(),
            _ => return Err(Error::NotChecked),
        };
        let context = crate::contact_verification::shared(dir)
            .map_err(|error| Error::Failed(error.to_string()))?;
        let mut context = context
            .lock()
            .map_err(|error| Error::Failed(error.to_string()))?;
        // Saving may fail after rename: invalidate before the attempt, even on an error.
        context.clear();
        save(dir, &network).map_err(|error| Error::Failed(error.to_string()))?;
        state.clear();
        Ok(network)
    }
}

/// Check active settings at launch without modifying either settings file.
///
/// # Errors
/// Returns storage/decoding errors; missing settings or an unsuccessful dial return false.
pub async fn connect(dir: &Path, wait: Duration) -> Result<bool, Error> {
    let Some(active) =
        network::load(dir, Settings::Active).map_err(|error| Error::Failed(error.to_string()))?
    else {
        return Ok(false);
    };
    let rows = seeds::check(&active, wait).await.unwrap_or_default();
    Ok(rows.iter().any(|row| matches!(row, Row::Connected(_))))
}

#[cfg(test)]
mod tests;
