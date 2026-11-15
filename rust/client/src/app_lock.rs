//! App-wide lock policy, independent of identity slots and installation node operations.

pub mod pin;
mod records;
mod state;
pub mod timer;

pub use state::AppLock;

/// Existing platform record format; selecting it does not migrate stored data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Android's PIN record and separate skipped/biometric markers.
    Android,
    /// iOS's single protected mode/PIN record.
    Ios,
}

/// Persisted app-wide choice, independent of the selected identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// No setup choice has been stored.
    New,
    /// Skipped or explicitly disabled.
    Off,
    /// Protected by the app PIN.
    Pin,
    /// Protected by platform biometric/device authentication.
    Biometric,
}

/// Result of an admitted and persisted PIN attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attempt {
    /// Correct PIN and persisted counter reset.
    Unlocked,
    /// Incorrect PIN with further attempts available.
    Wrong,
    /// Five failures reached; fresh platform authentication is required.
    DeviceCredentialRequired,
}

/// Protected storage entries; iOS uses only `Record`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    /// Decrypted legacy record; the platform protects it at rest.
    Record,
    /// Android first-launch Skip marker.
    Skipped,
    /// Android biometric-mode marker.
    Biometric,
}

/// Structural failure; no error contains PINs or verifier bytes.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Unsupported outer record, mode, counter or verifier framing.
    #[error("Invalid app-lock record")]
    InvalidRecord,
    /// A PIN attempt requires an enrolled PIN.
    #[error("App PIN is not enrolled")]
    NotEnrolled,
    /// Existing PINs require authenticated replacement.
    #[error("App PIN already exists")]
    AlreadyEnrolled,
    /// Recovery cannot reset a disabled or unconfigured lock.
    #[error("App lock is not enabled")]
    NotEnabled,
    /// Existing core verifier failure.
    #[error(transparent)]
    Pin(#[from] pin::Error),
    /// Protected local IO failed; implementations must omit credential material.
    #[error("{0}")]
    Storage(String),
    /// A panicking operation poisoned this handle's serialization lock.
    #[error("App lock state unavailable")]
    Poisoned,
}

/// Local protected IO only; implementations must not call back into the same app-lock handle.
/// Serialize access across handles targeting the same storage, and keep platform authentication
/// outside these callbacks. No callback receives a plaintext PIN.
pub trait Vault: Send + Sync {
    /// Read a decrypted record or an existing marker; absence is distinct from invalid storage.
    ///
    /// # Errors
    /// Return storage/keystore failures instead of treating them as missing entries.
    fn read(&self, entry: Entry) -> Result<Option<Vec<u8>>, Error>;

    /// Atomically and durably replace an entry. Markers use empty data.
    ///
    /// # Errors
    /// Return failure if durable completion is unconfirmed; callers do not report success.
    fn write(&self, entry: Entry, data: &[u8]) -> Result<(), Error>;

    /// Remove an entry; already absent is success.
    ///
    /// # Errors
    /// Return failure if removal cannot be completed.
    fn remove(&self, entry: Entry) -> Result<(), Error>;
}
