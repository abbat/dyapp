//! Native projection of core identity resolution without additional policy.

use crate::{IdentityKey, MobileSlot};
use dyapp_client::identity_slots as core;
use std::sync::Arc;

/// Stable recovery reason supplied by the shared registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileSlotRecovery {
    /// Key is confirmed missing.
    Missing,
    /// Platform or key access failed.
    Unavailable,
    /// Creation is incomplete.
    Incomplete,
    /// Removal is incomplete.
    Deleting,
    /// Metadata is damaged.
    Damaged,
    /// No candidate can recover an invalid active pointer.
    InvalidPointer,
}

impl From<core::RecoveryReason> for MobileSlotRecovery {
    fn from(reason: core::RecoveryReason) -> Self {
        match reason {
            core::RecoveryReason::Missing => Self::Missing,
            core::RecoveryReason::Unavailable => Self::Unavailable,
            core::RecoveryReason::Incomplete => Self::Incomplete,
            core::RecoveryReason::Deleting => Self::Deleting,
            core::RecoveryReason::Damaged => Self::Damaged,
            core::RecoveryReason::InvalidPointer => Self::InvalidPointer,
        }
    }
}

/// Existing identity resolution; Setup is never a request to generate a replacement key.
#[derive(uniffi::Enum)]
pub enum MobileSlotResolution {
    /// No identities or orphan credentials were found.
    Setup,
    /// Explicit selection is needed.
    Picker {
        /// Current candidates.
        slots: Vec<MobileSlot>,
    },
    /// Existing identity opened successfully.
    Ready {
        /// Verified location.
        slot: MobileSlot,
        /// Opaque existing key.
        key: Arc<IdentityKey>,
    },
    /// Explicit recovery is required.
    Recovery {
        /// Known slot or missing pointer target.
        id: Option<String>,
        /// Core recovery classification.
        reason: MobileSlotRecovery,
        /// Existing Peer ID when recoverable from an incomplete slot.
        peer_id: Option<String>,
    },
}

impl From<core::Resolution> for MobileSlotResolution {
    fn from(resolution: core::Resolution) -> Self {
        match resolution {
            core::Resolution::Setup => Self::Setup,
            core::Resolution::Picker(slots) => Self::Picker {
                slots: slots.into_iter().map(Into::into).collect(),
            },
            core::Resolution::Ready(slot, identity) => Self::Ready {
                slot: slot.into(),
                key: Arc::new(IdentityKey {
                    identity: *identity,
                }),
            },
            core::Resolution::Recovery {
                id,
                reason,
                peer_id,
            } => Self::Recovery {
                id,
                reason: reason.into(),
                peer_id,
            },
        }
    }
}
