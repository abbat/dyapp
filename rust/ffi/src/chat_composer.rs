//! Native conversion of the shared composer presentation policy.

use crate::ChatState;
use dyapp_client::chat_composer as composer;

/// Shared composer controls; authorization remains the responsibility of send operations.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatComposerPolicy {
    /// Whether the conversation phase permits typing.
    pub writable: bool,
    /// Whether the UTF-8 text exceeds the protocol limit.
    pub too_long: bool,
    /// Whether this phase and text permit submission.
    pub sendable: bool,
}

impl From<ChatState> for composer::State {
    fn from(value: ChatState) -> Self {
        match value {
            ChatState::None => Self::None,
            ChatState::RequestSent => Self::RequestSent,
            ChatState::RequestReceived => Self::RequestReceived,
            ChatState::Open => Self::Open,
            ChatState::Closed => Self::Closed,
        }
    }
}

/// Assess a native draft through shared core policy; no phase means loading.
#[uniffi::export]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned strings across the native boundary"
)]
pub fn chat_composer_policy(state: Option<ChatState>, text: String) -> ChatComposerPolicy {
    let policy = composer::assess(state.map(composer::State::from), &text);
    ChatComposerPolicy {
        writable: policy.writable,
        too_long: policy.too_long,
        sendable: policy.sendable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_states_and_utf8_text_use_the_shared_policy() {
        for (state, writable) in [
            (Some(ChatState::None), true),
            (Some(ChatState::RequestReceived), true),
            (Some(ChatState::Open), true),
            (Some(ChatState::RequestSent), false),
            (Some(ChatState::Closed), false),
            (None, false),
        ] {
            let policy = chat_composer_policy(state, "text".into());
            assert_eq!(policy.writable, writable);
            assert_eq!(policy.sendable, writable);
        }
        assert!(chat_composer_policy(Some(ChatState::Open), "Ж".repeat(32_768)).sendable);
        assert!(chat_composer_policy(None, "Ж".repeat(32_769)).too_long);
    }
}
