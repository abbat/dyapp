use super::MobileAppLockEntry;
use dyapp_client::app_lock as core;
use std::sync::Arc;

/// Foreign storage failures; reasons must never include PINs or verifier bytes.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppLockStorageError {
    /// Local protected IO/keystore failure.
    #[error("{reason}")]
    Failed {
        /// Structural cause suitable for diagnostics, without credentials.
        reason: String,
    },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for AppLockStorageError {
    fn from(error: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed {
            reason: error.reason,
        }
    }
}

/// Protected local IO only. Do not reenter the same app-lock object from a callback.
/// Writes must complete atomically and durably before returning; absence and failure differ.
#[uniffi::export(with_foreign)]
pub trait MobileAppLockStorage: Send + Sync {
    /// Read decrypted record bytes or an existing empty marker; None means confirmed absence.
    ///
    /// # Errors
    /// Return protected-file/keystore failures; never fabricate missing entries.
    fn read_entry(&self, entry: MobileAppLockEntry)
        -> Result<Option<Vec<u8>>, AppLockStorageError>;

    /// Atomically and durably write the core's record or empty marker, protecting records at rest.
    ///
    /// # Errors
    /// Return failure if completion is unconfirmed, without implying rollback.
    fn write_entry(
        &self,
        entry: MobileAppLockEntry,
        data: Vec<u8>,
    ) -> Result<(), AppLockStorageError>;

    /// Remove a local entry; confirmed absence is success.
    ///
    /// # Errors
    /// Return local IO failures without erasing other app or identity data.
    fn remove_entry(&self, entry: MobileAppLockEntry) -> Result<(), AppLockStorageError>;
}

pub(super) struct Storage(pub(super) Arc<dyn MobileAppLockStorage>);

impl core::Vault for Storage {
    fn read(&self, entry: core::Entry) -> Result<Option<Vec<u8>>, core::Error> {
        self.0
            .read_entry(entry.into())
            .map_err(|error| core::Error::Storage(error.to_string()))
    }

    fn write(&self, entry: core::Entry, data: &[u8]) -> Result<(), core::Error> {
        self.0
            .write_entry(entry.into(), data.to_vec())
            .map_err(|error| core::Error::Storage(error.to_string()))
    }

    fn remove(&self, entry: core::Entry) -> Result<(), core::Error> {
        self.0
            .remove_entry(entry.into())
            .map_err(|error| core::Error::Storage(error.to_string()))
    }
}
