//! Thin projection of shared identity-removal presentation policy.
use crate::MobileSlotRecovery;
use dyapp_client::identity_slots as core;

/// Read-only guidance; actual removal independently revalidates the current target and key.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct MobileIdentityRemovalReview {
    /// Whether the current inventoried resolution may offer confirmed removal.
    pub allowed: bool,
    /// Another ready readable local candidate has the same Peer ID.
    pub has_other_copy: bool,
}

impl From<core::RemovalReview> for MobileIdentityRemovalReview {
    fn from(value: core::RemovalReview) -> Self {
        Self {
            allowed: value.allowed,
            has_other_copy: value.has_other_copy,
        }
    }
}

impl From<MobileSlotRecovery> for core::RecoveryReason {
    fn from(reason: MobileSlotRecovery) -> Self {
        match reason {
            MobileSlotRecovery::Missing => Self::Missing,
            MobileSlotRecovery::Unavailable => Self::Unavailable,
            MobileSlotRecovery::Incomplete => Self::Incomplete,
            MobileSlotRecovery::Deleting => Self::Deleting,
            MobileSlotRecovery::Damaged => Self::Damaged,
            MobileSlotRecovery::InvalidPointer => Self::InvalidPointer,
        }
    }
}

/// Project a typed native resolution's removal hint; this never authorizes a mutation.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts an owned optional slot identifier across the native boundary"
)]
pub fn identity_removal_allowed(
    id: Option<String>,
    reason: Option<MobileSlotRecovery>,
    inventoried: bool,
) -> bool {
    core::removal_allowed(id.as_deref(), reason.map(Into::into), inventoried)
}
