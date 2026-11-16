//! Per-identity bootstrap settings and the seed check behind the first-run connection screen.
//!
//! Calls block their thread; the app runs them off the UI thread. A check saves the draft
//! first, so a crash keeps the user's rows, and only a check that reached a node of the network
//! can be activated.

use crate::slot_operation::SlotWorker;
use crate::MobileIdentityRegistry;
use dyapp_client::cancellation::Cancellation;
use dyapp_client::network_check::{Checks, Error as NetworkCheckError};
use dyapp_client::seeds::Row;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_p2p_net::seed::SeedError;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Seed rows and network name, as stored for one identity.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct NetworkSettings {
    /// `host:port` or `host:port/<peer ID>` rows, in the user's order.
    pub seeds: Vec<String>,
    /// The network name, empty for the public network.
    pub name: String,
}

impl From<NetworkSettings> for Network {
    fn from(settings: NetworkSettings) -> Self {
        Self {
            seeds: settings.seeds,
            name: settings.name,
        }
    }
}

impl From<Network> for NetworkSettings {
    fn from(network: Network) -> Self {
        Self {
            seeds: network.seeds,
            name: network.name,
        }
    }
}

/// Which of an identity's two settings files.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum NetworkFile {
    /// The rows being edited.
    Draft,
    /// The configuration in use, written only by [`ConnectionCheck::activate`].
    Active,
}

/// What one seed row turned out to be; the UI words it from the variant.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SeedRow {
    /// A node of this network answered.
    Connected {
        /// The node's peer ID.
        peer_id: String,
    },
    /// The row is not `host:port`.
    InvalidAddress,
    /// The part after `/` is not a peer ID.
    InvalidPeerId,
    /// The row names the same addresses as an earlier one.
    Duplicate,
    /// No dial succeeded in time.
    Unreachable {
        /// The last dial error, for logs; empty on timeout.
        reason: String,
    },
    /// A node answered with another key than the row pins.
    WrongPeer,
    /// A node answered from another network name.
    WrongNetwork,
}

impl From<Row> for SeedRow {
    fn from(row: Row) -> Self {
        match row {
            Row::Connected(peer) => Self::Connected {
                peer_id: peer.to_string(),
            },
            Row::Invalid(SeedError::Address) => Self::InvalidAddress,
            Row::Invalid(SeedError::PeerId) => Self::InvalidPeerId,
            Row::Duplicate => Self::Duplicate,
            Row::Unreachable(reason) => Self::Unreachable { reason },
            Row::WrongPin => Self::WrongPeer,
            Row::WrongNetwork => Self::WrongNetwork,
        }
    }
}

/// Why a check or activation gave no rows.
#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum CheckError {
    /// The draft has no seed rows.
    #[error("no seed rows")]
    Empty,
    /// The network name breaks ADR 0017.
    #[error("{reason}")]
    InvalidNetwork {
        /// Which rule the name breaks, for logs.
        reason: String,
    },
    /// The checker or the current attempt owner stopped the check.
    #[error("cancelled")]
    Cancelled,
    /// No check of this object reached a node of its network.
    #[error("no successful check")]
    NotChecked,
    /// Storage or the network stack failed.
    #[error("{reason}")]
    Failed {
        /// The cause, for logs.
        reason: String,
    },
}

impl From<NetworkCheckError> for CheckError {
    fn from(error: NetworkCheckError) -> Self {
        match error {
            NetworkCheckError::Empty => Self::Empty,
            NetworkCheckError::InvalidNetwork(reason) => Self::InvalidNetwork { reason },
            NetworkCheckError::Cancelled => Self::Cancelled,
            NetworkCheckError::NotChecked => Self::NotChecked,
            NetworkCheckError::Failed(reason) => Self::Failed { reason },
        }
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err transfers ownership of its source error into this adapter"
)]
fn failed(error: impl ToString) -> CheckError {
    CheckError::Failed {
        reason: error.to_string(),
    }
}

/// Reads one settings file of the identity folder `dir`; `None` when it was never saved.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if reading, decoding, or validating the selected settings file fails.
pub fn load_network(dir: String, file: NetworkFile) -> Result<Option<NetworkSettings>, CheckError> {
    let loaded = network::load(&PathBuf::from(dir), settings(file)).map_err(failed)?;
    Ok(loaded.map(Into::into))
}

/// Saves the draft of the identity folder `dir` without connecting.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if validating or writing the draft settings fails.
pub fn save_network_draft(dir: String, draft: NetworkSettings) -> Result<(), CheckError> {
    network::save(&PathBuf::from(dir), Settings::Draft, &draft.into()).map_err(failed)
}

fn settings(file: NetworkFile) -> Settings {
    match file {
        NetworkFile::Draft => Settings::Draft,
        NetworkFile::Active => Settings::Active,
    }
}

/// Checks and activates the drafts of one identity folder.
#[derive(uniffi::Object)]
pub struct ConnectionCheck {
    dir: PathBuf,
    runtime: tokio::runtime::Runtime,
    checks: Checks,
    binding: Option<SlotWorker>,
}

#[uniffi::export]
impl ConnectionCheck {
    /// A checker for the identity folder `dir`.
    #[uniffi::constructor]
    ///
    /// # Errors
    ///
    /// Returns an error if creating the private Tokio runtime fails.
    pub fn new(dir: String) -> Result<Arc<Self>, CheckError> {
        Self::create(dir, None)
    }

    /// Bind a checker to an existing slot; real runs and activation reacquire core protection.
    /// Idle handles retain no registry lock, and checks never operate the installation node.
    ///
    /// # Errors
    /// Returns identity admission or runtime failures without writing settings.
    #[uniffi::constructor]
    pub fn for_slot(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
    ) -> Result<Arc<Self>, CheckError> {
        let binding = SlotWorker::new(registry, id, expected_peer);
        let operation = binding.operation().map_err(failed)?;
        Self::create(operation.slot().directory, Some(binding))
    }

    /// Saves `draft`, then dials every row for up to `wait_ms` and reports each in order.
    /// Retry is another call; slot-bound checkers verify the existing key before saving.
    ///
    /// # Errors
    ///
    /// Returns an error if saving the draft fails, cancellation occurs, no seed
    /// rows exist, the network name is invalid, or swarm construction fails.
    /// The draft remains saved when a later check fails.
    pub fn run(&self, draft: NetworkSettings, wait_ms: u32) -> Result<Vec<SeedRow>, CheckError> {
        self.run_with_ready(draft, wait_ms, || {})
    }

    /// Stops a running check, which then fails with `Cancelled`; the saved files stay as they
    /// were.
    pub fn cancel(&self) {
        self.checks.cancel();
    }

    /// Makes the last draft that reached a node the identity's active configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if no successful check is recorded or saving active settings fails.
    pub fn activate(&self) -> Result<NetworkSettings, CheckError> {
        let _view = self
            .binding
            .as_ref()
            .map(SlotWorker::view_transition)
            .transpose()
            .map_err(failed)?;
        let _operation = self
            .binding
            .as_ref()
            .map(SlotWorker::operation)
            .transpose()
            .map_err(failed)?;
        self.checks
            .activate(&self.dir)
            .map(Into::into)
            .map_err(Into::into)
    }
}

impl ConnectionCheck {
    fn create(dir: String, binding: Option<SlotWorker>) -> Result<Arc<Self>, CheckError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            dir: dir.into(),
            runtime,
            checks: Checks::default(),
            binding,
        }))
    }

    pub(crate) fn run_attempt(
        &self,
        draft: NetworkSettings,
        wait_ms: u32,
        cancellation: &Cancellation,
    ) -> Result<Vec<SeedRow>, CheckError> {
        self.run_attempt_with_ready(draft, wait_ms, cancellation, || {})
    }

    fn run_with_ready(
        &self,
        draft: NetworkSettings,
        wait_ms: u32,
        ready: impl FnOnce(),
    ) -> Result<Vec<SeedRow>, CheckError> {
        self.run_attempt_with_ready(draft, wait_ms, &Cancellation::new(), ready)
    }

    // Readiness is after legacy registration; the attempt owner exists before admission.
    fn run_attempt_with_ready(
        &self,
        draft: NetworkSettings,
        wait_ms: u32,
        cancellation: &Cancellation,
        ready: impl FnOnce(),
    ) -> Result<Vec<SeedRow>, CheckError> {
        if cancellation.is_cancelled() {
            return Err(CheckError::Cancelled);
        }
        let _operation = self
            .binding
            .as_ref()
            .map(SlotWorker::operation)
            .transpose()
            .map_err(failed)?;
        if cancellation.is_cancelled() {
            return Err(CheckError::Cancelled);
        }
        self.runtime
            .block_on(self.checks.check(
                self.dir.clone(),
                draft.into(),
                Duration::from_millis(wait_ms.into()),
                cancellation,
                || {
                    ready();
                    Ok(false)
                },
            ))
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }
}

#[cfg(test)]
pub(crate) mod tests;
