use dyapp_client::app_lock as core;

/// Existing protected record format; selecting it does not migrate stored locks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileAppLockFormat {
    /// Android PIN file and separate markers.
    Android,
    /// iOS single Keychain item.
    Ios,
}

impl From<MobileAppLockFormat> for core::Format {
    fn from(format: MobileAppLockFormat) -> Self {
        match format {
            MobileAppLockFormat::Android => Self::Android,
            MobileAppLockFormat::Ios => Self::Ios,
        }
    }
}

/// App-wide choice returned by the common core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileAppLockMode {
    /// First launch with no persisted choice.
    New,
    /// Skipped or explicitly disabled.
    Off,
    /// App PIN authentication.
    Pin,
    /// Platform biometric/device authentication.
    Biometric,
}

impl From<core::Mode> for MobileAppLockMode {
    fn from(mode: core::Mode) -> Self {
        match mode {
            core::Mode::New => Self::New,
            core::Mode::Off => Self::Off,
            core::Mode::Pin => Self::Pin,
            core::Mode::Biometric => Self::Biometric,
        }
    }
}

/// Outcome of one persisted core PIN attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobilePinAttempt {
    /// Correct PIN and completed counter reset.
    Unlocked,
    /// Wrong PIN with attempts remaining.
    Wrong,
    /// Five failures; fresh OS authentication is required.
    DeviceCredentialRequired,
}

impl From<core::Attempt> for MobilePinAttempt {
    fn from(attempt: core::Attempt) -> Self {
        match attempt {
            core::Attempt::Unlocked => Self::Unlocked,
            core::Attempt::Wrong => Self::Wrong,
            core::Attempt::DeviceCredentialRequired => Self::DeviceCredentialRequired,
        }
    }
}

/// Local protected IO keys, never identity-slot paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileAppLockEntry {
    /// Existing decrypted outer record, protected at rest by the platform.
    Record,
    /// Android's first-launch Skip marker.
    Skipped,
    /// Android's biometric marker.
    Biometric,
}

impl From<core::Entry> for MobileAppLockEntry {
    fn from(entry: core::Entry) -> Self {
        match entry {
            core::Entry::Record => Self::Record,
            core::Entry::Skipped => Self::Skipped,
            core::Entry::Biometric => Self::Biometric,
        }
    }
}
