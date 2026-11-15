//! Thin common native projection of the core's monotonic app-session timer.

use crate::CoreError;
use dyapp_client::app_lock::timer::{self, Timer};
use std::sync::{Arc, Mutex};

/// Lifecycle or successfully authorized credential event; authorization stays with the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileAppLockEvent {
    /// App backgrounded.
    Background,
    /// App returned to the foreground.
    Foreground,
    /// Actual user interaction.
    Interaction,
    /// Successful foreground authentication.
    Unlocked,
    /// Successful enrollment or authorized protected-mode change.
    Enrolled,
    /// Authorized removal of protection.
    Cleared,
}

impl From<MobileAppLockEvent> for timer::Event {
    fn from(event: MobileAppLockEvent) -> Self {
        match event {
            MobileAppLockEvent::Background => Self::Background,
            MobileAppLockEvent::Foreground => Self::Foreground,
            MobileAppLockEvent::Interaction => Self::Interaction,
            MobileAppLockEvent::Unlocked => Self::Unlocked,
            MobileAppLockEvent::Enrolled => Self::Enrolled,
            MobileAppLockEvent::Cleared => Self::Cleared,
        }
    }
}

/// Core-derived ephemeral presentation state; no credentials or identity information.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct MobileAppLockSnapshot {
    /// Protection is enabled.
    pub enabled: bool,
    /// Private content requires authentication.
    pub locked: bool,
    /// Background snapshots must be covered.
    pub obscured: bool,
    /// Remaining monotonic milliseconds; absent when locked or disabled.
    pub remaining_ms: Option<u64>,
}

impl From<timer::Snapshot> for MobileAppLockSnapshot {
    fn from(state: timer::Snapshot) -> Self {
        Self {
            enabled: state.enabled,
            locked: state.locked,
            obscured: state.obscured,
            remaining_ms: state.remaining_ms,
        }
    }
}

/// One app-session timer; platforms own its lifetime and supply their monotonic clock.
#[derive(uniffi::Object)]
pub struct MobileAppLockTimer {
    core: Mutex<Timer>,
}

#[uniffi::export]
impl MobileAppLockTimer {
    /// Start a fresh timer without reading or writing any protected storage.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(enabled: bool, now_ms: u64) -> Arc<Self> {
        Arc::new(Self {
            core: Mutex::new(Timer::new(enabled, now_ms)),
        })
    }

    /// Observe the core's current lock and next deadline.
    ///
    /// # Errors
    /// Returns synchronization failure; callers must keep private content covered.
    pub fn snapshot(&self, now_ms: u64) -> Result<MobileAppLockSnapshot, CoreError> {
        self.core
            .lock()
            .map(|mut core| core.snapshot(now_ms).into())
            .map_err(|error| CoreError::Failed(error.to_string()))
    }

    /// Forward one event; all expiration, ordering and transition rules remain in the core.
    ///
    /// # Errors
    /// Returns synchronization, backwards clock or invalid unlock errors.
    pub fn transition(
        &self,
        event: MobileAppLockEvent,
        now_ms: u64,
    ) -> Result<MobileAppLockSnapshot, CoreError> {
        self.core
            .lock()
            .map_err(|error| CoreError::Failed(error.to_string()))?
            .apply(event.into(), now_ms)
            .map(Into::into)
            .map_err(|error| CoreError::Failed(error.to_string()))
    }
}

#[cfg(test)]
mod tests;
