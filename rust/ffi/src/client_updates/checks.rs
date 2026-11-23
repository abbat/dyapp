//! Explicit checker bridge; exporting it does not activate any client lifecycle request.
use super::{core, MobileClientUpdates, MobileUpdateState};

/// Native lifecycle intent; cadence, coalescing and backoff remain in shared core.
#[derive(Clone, Copy, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateTrigger {
    /// Launch or return from suspension.
    LaunchOrResume,
    /// Scheduled wake.
    Timer,
    /// Connectivity returned.
    NetworkRestored,
    /// Explicit owner action.
    Manual,
}

impl From<MobileUpdateTrigger> for core::Trigger {
    fn from(value: MobileUpdateTrigger) -> Self {
        match value {
            MobileUpdateTrigger::LaunchOrResume => Self::LaunchOrResume,
            MobileUpdateTrigger::Timer => Self::Timer,
            MobileUpdateTrigger::NetworkRestored => Self::NetworkRestored,
            MobileUpdateTrigger::Manual => Self::Manual,
        }
    }
}

/// Core failure categories, separate from the retained trusted metadata.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateCheckFailure {
    /// Request, response or feed parsing failed; details are diagnostic text, not markup or a URL action.
    Transport { details: String },
    /// Shared snapshot validation rejected the candidate.
    Rejected { details: String },
    /// Completion time was unavailable.
    Clock,
}

/// A cached offer is not evidence that the current request succeeded.
#[derive(Clone, Debug, Eq, PartialEq, uniffi::Enum)]
pub enum MobileUpdateCheck {
    /// Another caller owns the independent update lease; no request was started here.
    Busy,
    /// Cadence refused a request; this is existing metadata, not a successful new check.
    NotDue { state: MobileUpdateState },
    /// Validated completion was committed by shared core.
    Validated { state: MobileUpdateState },
    /// Failed completion/backoff was committed while preserving earlier trusted metadata.
    Failed {
        state: MobileUpdateState,
        failure: MobileUpdateCheckFailure,
    },
}

pub(super) fn project(
    owner: &MobileClientUpdates,
    outcome: &core::checker::Outcome,
) -> MobileUpdateCheck {
    use core::checker::{Failure, Outcome};
    match outcome {
        Outcome::Busy => MobileUpdateCheck::Busy,
        Outcome::NotDue(snapshot) => MobileUpdateCheck::NotDue {
            state: owner.project(snapshot),
        },
        Outcome::Validated(snapshot) => MobileUpdateCheck::Validated {
            state: owner.project(snapshot),
        },
        Outcome::Failed { snapshot, reason } => MobileUpdateCheck::Failed {
            state: owner.project(snapshot),
            failure: match reason {
                Failure::Transport(error) => MobileUpdateCheckFailure::Transport {
                    details: error.to_string(),
                },
                Failure::Rejected(error) => MobileUpdateCheckFailure::Rejected {
                    details: error.to_string(),
                },
                Failure::Clock => MobileUpdateCheckFailure::Clock,
            },
        },
    }
}
