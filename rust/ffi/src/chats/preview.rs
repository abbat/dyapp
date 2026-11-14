//! `UniFFI` shape of the shared core Chats-list projection.
use dyapp_client::chat_preview as core;

/// Localized labels are supplied by the UI; variable content is already bounded.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatPreviewBody {
    /// No last item.
    Empty,
    /// Deleted message.
    Deleted,
    /// Bounded current text.
    Text {
        /// Plain text, including any core-owned omission marker.
        text: String,
    },
    /// Photo, without fetching media.
    Photo,
    /// Sticker fallback metadata.
    Sticker {
        /// Bounded emoji.
        emoji: String,
        /// Bounded pack label.
        pack_name: String,
    },
    /// A validated shared profile link.
    Profile {
        /// Canonical target Peer ID.
        peer_id: String,
    },
}

impl From<core::ChatPreviewBody> for ChatPreviewBody {
    fn from(body: core::ChatPreviewBody) -> Self {
        match body {
            core::ChatPreviewBody::Empty => Self::Empty,
            core::ChatPreviewBody::Deleted => Self::Deleted,
            core::ChatPreviewBody::Text { text } => Self::Text { text },
            core::ChatPreviewBody::Photo => Self::Photo,
            core::ChatPreviewBody::Sticker { emoji, pack_name } => {
                Self::Sticker { emoji, pack_name }
            }
            core::ChatPreviewBody::Profile { peer_id } => Self::Profile { peer_id },
        }
    }
}

/// Already-selected newest item; not notification/unread policy.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatListPreview {
    /// Typed bounded content.
    pub body: ChatPreviewBody,
    /// Forwarded live text/photo.
    pub forwarded: bool,
    /// Original wall time, or absent for legacy/empty rows.
    pub sent_at_ms: Option<u64>,
}

impl From<core::ChatListPreview> for ChatListPreview {
    fn from(preview: core::ChatListPreview) -> Self {
        Self {
            body: preview.body.into(),
            forwarded: preview.forwarded,
            sent_at_ms: preview.sent_at_ms,
        }
    }
}
