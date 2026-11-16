//! Native presentation of shared receiver phases and retained installation boundaries.

use crate::MobileSlotSelection;
use dyapp_client::identity_slots as core;

/// Shared receiver progress; this never asserts rollback or destination lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileLinkPhase {
    /// Another poll may start.
    Waiting,
    /// One worker is receiving.
    Receiving,
    /// Cancellation won before installation admission.
    Cancelled,
    /// Local writing has started and cannot be rolled back by cancellation.
    Installing,
    /// The original installation returned success.
    Completed,
    /// The original installation returned an error.
    Failed,
    /// There was no normal completion; reconcile inventory before recovery.
    Interrupted,
    /// The invitation's frontend profile generation changed.
    Invalidated,
}

impl From<core::LinkPhase> for MobileLinkPhase {
    fn from(phase: core::LinkPhase) -> Self {
        match phase {
            core::LinkPhase::Waiting => Self::Waiting,
            core::LinkPhase::Receiving => Self::Receiving,
            core::LinkPhase::Cancelled => Self::Cancelled,
            core::LinkPhase::Installing => Self::Installing,
            core::LinkPhase::Completed => Self::Completed,
            core::LinkPhase::Failed => Self::Failed,
            core::LinkPhase::Interrupted => Self::Interrupted,
            core::LinkPhase::Invalidated => Self::Invalidated,
        }
    }
}

/// Original installation boundary, not a promise of pending state or rollback.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileLinkFailure {
    /// No normal writer completion is known.
    Unconfirmed {
        /// Original diagnostic.
        detail: String,
    },
    /// No destination reservation began.
    Admission {
        /// Original diagnostic.
        detail: String,
    },
    /// Reservation failed; no exact destination selector is confirmed.
    Reservation {
        /// Original diagnostic.
        detail: String,
    },
    /// An exact destination was reserved; re-read its actual lifecycle.
    Destination {
        /// Original reserved selector, never inferred from inventory changes.
        slot_id: String,
        /// Original diagnostic.
        detail: String,
    },
}

impl From<&core::LinkFailure> for MobileLinkFailure {
    fn from(failure: &core::LinkFailure) -> Self {
        match failure {
            core::LinkFailure::Unconfirmed(detail) => Self::Unconfirmed {
                detail: detail.clone(),
            },
            core::LinkFailure::Admission(detail) => Self::Admission {
                detail: detail.clone(),
            },
            core::LinkFailure::Reservation(detail) => Self::Reservation {
                detail: detail.clone(),
            },
            core::LinkFailure::Destination { slot_id, detail } => Self::Destination {
                slot_id: slot_id.clone(),
                detail: detail.clone(),
            },
        }
    }
}

/// Cached original writer receipt; selection may have changed since its completion.
#[derive(Clone, uniffi::Enum)]
pub enum MobileLinkReceipt {
    /// Opaque verified key and actual selection-durability result.
    Installed {
        /// Original committed selection.
        selection: MobileSlotSelection,
    },
    /// Original failure boundary.
    Failed {
        /// No implied rollback or automatic retry.
        failure: MobileLinkFailure,
    },
}
