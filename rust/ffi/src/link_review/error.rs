use dyapp_client::publish::{AdoptionConfirmError, Failure};

/// Recovery policy refusals are distinct from backend failures and cannot be retried blindly.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LinkReviewError {
    /// Another local/link/remote context or device requires a fresh comparison.
    #[error("linked-device recovery needs a fresh review")]
    ReviewRequired,
    /// A field decision is missing, duplicated or was never offered.
    #[error("invalid field selection: {detail}")]
    Selection {
        /// Core validation detail.
        detail: String,
    },
    /// The selected profile violates the shared wire contract.
    #[error("invalid selected profile: {detail}")]
    Profile {
        /// Core validation detail.
        detail: String,
    },
    /// This original device is absent from remote authorization and cannot restore itself.
    #[error("the original device is not listed by the remote profile")]
    DeviceNotListed,
    /// Admission, credential, storage or network failure; no automatic field decision is implied.
    #[error("{detail}")]
    Failed {
        /// Backend diagnostic, not a stable policy code.
        detail: String,
    },
}
pub(super) fn failed(error: impl std::fmt::Display) -> LinkReviewError {
    LinkReviewError::Failed {
        detail: error.to_string(),
    }
}
impl From<crate::CoreError> for LinkReviewError {
    fn from(error: crate::CoreError) -> Self {
        failed(error)
    }
}
impl From<Failure> for LinkReviewError {
    fn from(error: Failure) -> Self {
        match error {
            Failure::AdoptionReviewRequired => Self::ReviewRequired,
            error => failed(error),
        }
    }
}
impl From<AdoptionConfirmError> for LinkReviewError {
    fn from(error: AdoptionConfirmError) -> Self {
        match error {
            AdoptionConfirmError::Selection(error) => Self::Selection {
                detail: error.to_string(),
            },
            AdoptionConfirmError::Profile(error) => Self::Profile {
                detail: error.to_string(),
            },
            AdoptionConfirmError::DeviceNotListed => Self::DeviceNotListed,
            AdoptionConfirmError::Recovery(error) => error.into(),
            AdoptionConfirmError::Credential(error) => failed(error),
        }
    }
}
