//! Thin native projection of the core's committed submission boundary.

use super::failed;
use crate::CoreError;
use dyapp_client::session::{Error, Submission};

/// A committed operation's node delivery result, never a peer receipt or read indication.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatSubmission {
    /// All required node writes were accepted.
    Accepted {
        /// The stable logical message or operation ID.
        id: Vec<u8>,
    },
    /// The operation is stored and retries retain its identity and prepared bytes.
    Queued {
        /// The stable logical message or operation ID.
        id: Vec<u8>,
        /// The immediate delivery failure; not a reason to submit another copy.
        detail: String,
    },
}

impl ChatSubmission {
    pub(super) fn from_core(value: Submission, error: fn(Error) -> CoreError) -> Self {
        match value {
            Submission::Accepted { id } => Self::Accepted { id: id.to_vec() },
            Submission::Queued { id, error: cause } => Self::Queued {
                id: id.to_vec(),
                detail: error(cause).to_string(),
            },
        }
    }

    pub(super) fn into_accepted(self) -> Result<Vec<u8>, CoreError> {
        match self {
            Self::Accepted { id } => Ok(id),
            Self::Queued { detail, .. } => Err(failed(detail)),
        }
    }
}

impl From<Submission> for ChatSubmission {
    fn from(value: Submission) -> Self {
        Self::from_core(value, failed)
    }
}

#[cfg(test)]
mod tests;
