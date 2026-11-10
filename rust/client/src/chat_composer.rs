//! Composer presentation policy; sending still enforces authorization and persistence separately.

use crate::chat::MAX_TEXT_BYTES;
use crate::store::Conversation;

/// Identifier-free conversation phase for client presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No text and no match yet.
    None,
    /// The owner's first text waits for a reply.
    RequestSent,
    /// The peer's first text can be answered.
    RequestReceived,
    /// An open conversation.
    Open,
    /// An ended match.
    Closed,
}

impl From<Conversation> for State {
    fn from(value: Conversation) -> Self {
        match value {
            Conversation::None => Self::None,
            Conversation::RequestSent(_) => Self::RequestSent,
            Conversation::RequestReceived(_) => Self::RequestReceived,
            Conversation::Open => Self::Open,
            Conversation::Closed => Self::Closed,
        }
    }
}

/// Current composer controls, computed without network or storage work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assessment {
    /// Whether the phase permits typing; a missing phase means loading.
    pub writable: bool,
    /// Whether UTF-8 text exceeds the protocol limit, regardless of phase.
    pub too_long: bool,
    /// Whether nonblank bounded text can be submitted in this phase.
    pub sendable: bool,
}

/// Assess text using protocol bytes and the Linux desktop composer's ECMAScript trim semantics.
#[must_use]
pub fn assess(state: Option<State>, text: &str) -> Assessment {
    let writable = matches!(
        state,
        Some(State::None | State::RequestReceived | State::Open)
    );
    let too_long = text.len() > MAX_TEXT_BYTES;
    let nonblank = !crate::text_input::trim(text).is_empty();
    Assessment {
        writable,
        too_long,
        sendable: writable && nonblank && !too_long,
    }
}

#[cfg(test)]
mod tests;
