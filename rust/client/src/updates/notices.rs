//! Durable notice actions independent of profile selection and the HTTP attempt lease.
use super::{availability::Offer, feed::Platform, snapshot::Snapshot, storage, Version};
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

/// Select and claim the current eligible OS offer in the same transaction.
/// Native permission must already be available. Only the returned committed offer
/// permits an OS request; native code does not select or compare candidate versions.
///
/// # Errors
/// Returns persistence or validation failure without returning a provisional offer.
pub fn claim_pending_notification(
    root: &Path,
    platform: Platform,
    installed: Version,
) -> Result<Option<Offer>, storage::Error> {
    let (_, offer) = storage::change_state(root, |snapshot| {
        let Some(offer) = snapshot.notice(platform, installed) else {
            return Ok(None);
        };
        Ok(snapshot
            .claim_notification(platform, installed, offer.version())
            .then_some(offer))
    })?;
    Ok(offer)
}

#[cfg(test)]
mod tests;
