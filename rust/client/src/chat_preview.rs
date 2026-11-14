//! Bounded local Chats-list content shared by desktop JSON and mobile FFI.
//! This is not notification policy: read messages still have a list preview.

use crate::store::messages::ChatMessage;
use crate::store::ChatRow;
use dyapp_identity::profile_link::parse_profile_link;

#[cfg(test)]
mod tests;
mod text;

/// Newest visible body, without localized labels or fetching media.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatPreviewBody {
    /// A cleared or still empty conversation.
    Empty,
    /// A deletion placeholder; never retained older text.
    Deleted,
    /// Plain bounded text, with an ellipsis marker only when content was omitted.
    Text {
        /// At most 80 extended graphemes and 512 UTF-8 bytes, marker included.
        text: String,
    },
    /// A photo marker, without downloading its preview or original.
    Photo,
    /// Validated sticker metadata, with bounded variable display content.
    Sticker {
        /// The sticker's grapheme, or a marker if pathological input exceeds the byte bound.
        emoji: String,
        /// Pack label; clients supply the localized sticker label.
        #[serde(rename = "packName")]
        pack_name: String,
    },
    /// A shared profile; clients resolve its local display label.
    Profile {
        /// Validated profile-link Peer ID, not the encoded wire string.
        #[serde(rename = "peerId")]
        peer_id: String,
    },
}

/// Presentation of the Store's already-selected newest row item.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatListPreview {
    /// Typed content; labels and localization stay in UI adapters.
    pub body: ChatPreviewBody,
    /// Whether the newest live text or photo is forwarded.
    pub forwarded: bool,
    /// Original message time; legacy or empty rows have no time.
    pub sent_at_ms: Option<u64>,
}

/// Project the newest marker chosen by the Store, without mutating history.
///
/// Newer media must not borrow retained older text. This intentionally ignores
/// unread/notification settings: viewing a message does not erase its list summary.
#[must_use]
pub fn for_row(row: &ChatRow) -> ChatListPreview {
    if let Some(sticker) = &row.last_sticker {
        return ChatListPreview {
            body: ChatPreviewBody::Sticker {
                emoji: text::bounded(&sticker.sticker.emoji),
                pack_name: text::bounded(&sticker.sticker.pack_name),
            },
            forwarded: false,
            sent_at_ms: Some(sticker.sent_at_ms()),
        };
    }
    if let Some(photo) = &row.last_attachment {
        return ChatListPreview {
            body: ChatPreviewBody::Photo,
            forwarded: photo.forwarded,
            sent_at_ms: Some(photo.sent_at_ms()),
        };
    }
    from_text(row.last_message.as_ref())
}

fn from_text(message: Option<&ChatMessage>) -> ChatListPreview {
    let Some(message) = message else {
        return ChatListPreview {
            body: ChatPreviewBody::Empty,
            forwarded: false,
            sent_at_ms: None,
        };
    };
    let body = if message.current.deleted {
        ChatPreviewBody::Deleted
    } else {
        message
            .current
            .text
            .as_deref()
            .map_or(ChatPreviewBody::Empty, |bytes| {
                let content = String::from_utf8_lossy(bytes);
                parse_profile_link(&content).map_or_else(
                    || ChatPreviewBody::Text {
                        text: text::bounded(&content),
                    },
                    |peer| ChatPreviewBody::Profile {
                        peer_id: peer.to_owned(),
                    },
                )
            })
    };
    let forwarded =
        message.forwarded && !matches!(body, ChatPreviewBody::Empty | ChatPreviewBody::Deleted);
    ChatListPreview {
        body,
        forwarded,
        sent_at_ms: message
            .original_hlc
            .map(dyapp_messaging::Timestamp::wall_ms),
    }
}
