//! Durable notice actions independent of profile selection and the HTTP attempt lease.
use super::{feed::Platform, snapshot::Snapshot, storage, Version};
use std::path::Path;

/// Dismiss only the exact displayed offer; the Settings badge remains available.
///
/// # Errors
/// Returns persistence or validation failure without acknowledging the dismissal.
pub fn dismiss(
    root: &Path,
    platform: Platform,
    installed: Version,
    expected: Version,
) -> Result<(Snapshot, bool), storage::Error> {
    storage::change_state(root, |snapshot| {
        Ok(snapshot.dismiss_notice(platform, installed, expected))
    })
}

/// Claim one OS alert only after native permission is confirmed.
/// Only a committed true result permits requesting the native notification.
///
/// # Errors
/// Returns persistence or validation failure; no OS request is permitted on error.
pub fn claim_notification(
    root: &Path,
    platform: Platform,
    installed: Version,
    expected: Version,
) -> Result<(Snapshot, bool), storage::Error> {
    storage::change_state(root, |snapshot| {
        Ok(snapshot.claim_notification(platform, installed, expected))
    })
}

#[cfg(test)]
mod tests;
