//! Shared presentation hint; actual removal independently validates the current key and files.
use super::{RecoveryReason, LEGACY};

/// Whether this inventoried resolution can offer the existing confirmed-removal flow.
///
/// This does not authorize deletion. Legacy roots, absent candidates and unreadable/damaged keys
/// remain unavailable; missing keys and interrupted creation/removal retain explicit cleanup.
#[must_use]
pub fn removal_allowed(
    id: Option<&str>,
    reason: Option<RecoveryReason>,
    inventoried: bool,
) -> bool {
    inventoried
        && id.is_some_and(|id| id != LEGACY)
        && matches!(
            reason,
            None | Some(
                RecoveryReason::Missing | RecoveryReason::Incomplete | RecoveryReason::Deleting
            )
        )
}

#[cfg(test)]
#[path = "removal_policy/tests.rs"]
mod tests;
