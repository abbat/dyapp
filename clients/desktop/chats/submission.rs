//! Serialize the core's committed operation identity and immediate delivery result.

use dyapp_client::session::Submission;

/// A committed operation; a delivery error leaves its exact outbound writes queued in the core.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Submitted {
    id: [u8; 16],
    queued: Option<String>,
}

impl From<Submission> for Submitted {
    fn from(submission: Submission) -> Self {
        match submission {
            Submission::Accepted { id } => Self { id, queued: None },
            Submission::Queued { id, error } => Self {
                id,
                queued: Some(error.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Submission, Submitted};

    #[test]
    fn queued_operation_keeps_committed_id_for_ipc() {
        let id = [7; 16];
        let queued = Submitted::from(Submission::Queued {
            id,
            error: dyapp_client::session::Error::Invalidated,
        });
        assert_eq!(
            serde_json::to_value(&queued).unwrap(),
            serde_json::json!({ "id": id, "queued": "MLS session must be reopened" })
        );
        let accepted = Submitted::from(Submission::Accepted { id });
        assert_eq!(
            serde_json::to_value(&accepted).unwrap(),
            serde_json::json!({ "id": id, "queued": null })
        );
    }
}
