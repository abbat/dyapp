//! Thin common native ownership of the shared app-wide lock state machine.

mod storage;
mod types;

pub use storage::{AppLockStorageError, MobileAppLockStorage};
pub use types::{MobileAppLockEntry, MobileAppLockFormat, MobileAppLockMode, MobilePinAttempt};

use crate::CoreError;
use dyapp_client::app_lock::AppLock;
use std::sync::Arc;

/// One serialized core lock handle, independent of profiles and installation node lifecycle.
/// Call blocking IO/KDF methods off the UI thread; retain ownership through actual completion.
#[derive(uniffi::Object)]
pub struct MobileAppLock {
    core: AppLock,
}

#[uniffi::export]
impl MobileAppLock {
    /// Retain protected IO without reading, writing or migrating existing records.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(format: MobileAppLockFormat, storage: Arc<dyn MobileAppLockStorage>) -> Arc<Self> {
        Arc::new(Self {
            core: AppLock::new(format.into(), Arc::new(storage::Storage(storage))),
        })
    }

    /// Read the existing choice; invalid storage is never fresh setup.
    ///
    /// # Errors
    /// Returns core record, protected IO or synchronization failure.
    pub fn mode(&self) -> Result<MobileAppLockMode, CoreError> {
        self.core.mode().map(Into::into).map_err(failed)
    }

    /// Enroll without replacing an existing PIN.
    ///
    /// # Errors
    /// Returns core enrollment, PIN, record or protected IO failure.
    pub fn enroll(&self, pin: String) -> Result<(), CoreError> {
        self.core.enroll(pin).map_err(failed)
    }

    /// Enroll only while the authoritative core mode is New/Off.
    ///
    /// # Errors
    /// Returns already-enabled, PIN, record or protected IO failure.
    pub fn enroll_initial(&self, pin: String) -> Result<(), CoreError> {
        self.core.enroll_initial(pin).map_err(failed)
    }

    /// Consume a persisted attempt and report only the core's admitted result.
    ///
    /// # Errors
    /// Returns core attempt, PIN, record or protected IO failure.
    pub fn attempt(&self, pin: String) -> Result<MobilePinAttempt, CoreError> {
        self.core.attempt(pin).map(Into::into).map_err(failed)
    }

    /// Authenticate with the current PIN before changing or disabling it.
    ///
    /// # Errors
    /// Returns core authentication/recovery or protected IO failure; no automatic retry.
    pub fn change(
        &self,
        current: String,
        new_pin: Option<String>,
    ) -> Result<MobilePinAttempt, CoreError> {
        self.core
            .change(current, new_pin)
            .map(Into::into)
            .map_err(failed)
    }

    /// Invoke only after fresh platform OS authentication; core cannot prove it occurred.
    ///
    /// # Errors
    /// Returns core recovery, PIN, record or protected IO failure.
    pub fn reset_after_device_credential(&self, new_pin: Option<String>) -> Result<(), CoreError> {
        self.core
            .reset_after_device_credential(new_pin)
            .map_err(failed)
    }

    /// Store Skip without disabling an existing lock.
    ///
    /// # Errors
    /// Returns core record or protected IO failure.
    pub fn skip(&self) -> Result<(), CoreError> {
        self.core.skip().map_err(failed)
    }

    /// Choose biometric mode on fresh setup, while off or after fresh platform authentication.
    ///
    /// # Errors
    /// Returns core record or protected IO failure.
    pub fn use_biometric(&self) -> Result<(), CoreError> {
        self.core.use_biometric().map_err(failed)
    }

    /// Choose biometric mode only while the authoritative core mode is New/Off.
    ///
    /// # Errors
    /// Returns already-enabled, record or protected IO failure.
    pub fn use_biometric_initial(&self) -> Result<(), CoreError> {
        self.core.use_biometric_initial().map_err(failed)
    }
}

fn failed(error: dyapp_client::app_lock::Error) -> CoreError {
    match error {
        dyapp_client::app_lock::Error::Storage(reason) => CoreError::Failed(reason),
        other => CoreError::Failed(other.to_string()),
    }
}

#[cfg(test)]
mod tests;
