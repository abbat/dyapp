//! Like submission and subsequent projection reads have independent outcomes.

use super::{failed, ChatLikes, ChatSubmission};
use crate::CoreError;
use dyapp_client::session::{InteractionSubmission, Submission};

/// Availability of the local like projection after an interaction attempt.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatLikeState {
    /// The state was read successfully.
    Ready {
        /// The current core projection.
        likes: ChatLikes,
    },
    /// A read failure does not undo an already committed signal.
    Unavailable {
        /// The projection read failure.
        detail: String,
    },
}

/// A successful core attempt; pre-commit failures remain native errors.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatLikeAttempt {
    /// The actual committed event; absent when core deliberately created no event.
    pub submission: Option<ChatSubmission>,
    /// A later state read, independent of the attempt's commitment and delivery.
    pub state: ChatLikeState,
}

impl ChatLikeAttempt {
    pub(super) fn new(
        value: Option<InteractionSubmission>,
        state: Result<ChatLikes, CoreError>,
    ) -> Self {
        let submission = value.map(|value| {
            let id = value.event.event_id;
            ChatSubmission::from(match value.delivery {
                Ok(()) => Submission::Accepted { id },
                Err(error) => Submission::Queued { id, error },
            })
        });
        let state = match state {
            Ok(likes) => ChatLikeState::Ready { likes },
            Err(error) => ChatLikeState::Unavailable {
                detail: error.to_string(),
            },
        };
        Self { submission, state }
    }

    pub(super) fn into_accepted(self) -> Result<ChatLikes, CoreError> {
        self.submission
            .map(ChatSubmission::into_accepted)
            .transpose()?;
        match self.state {
            ChatLikeState::Ready { likes } => Ok(likes),
            ChatLikeState::Unavailable { detail } => Err(failed(detail)),
        }
    }
}

#[cfg(test)]
mod tests;
