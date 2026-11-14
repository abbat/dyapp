//! Shared private notification content; OS posting, localized labels and markup stay in adapters.
use crate::store::{ChatRow, Conversation, NoticeContent};

/// Maximum Unicode scalar values in a notification body, before platform-specific escaping.
pub const TEXT_CHARS: usize = 200;

/// Allowed content, without media descriptors, pack metadata or localized platform strings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Preview {
    /// Keep the platform's fixed new-message body.
    #[default]
    Empty,
    /// Current unread text.
    Text(String),
    /// Current unread sticker's validated grapheme, never its pack name.
    Sticker(String),
}
impl Preview {
    /// Render plain bounded text with a trusted localized sticker label. Adapters escape markup
    /// only if their OS API interprets it; escaping does not belong in the shared policy.
    #[must_use]
    pub fn plain_text(self, sticker_label: &str) -> String {
        let text = match self {
            Self::Empty => String::new(),
            Self::Text(text) => text,
            Self::Sticker(emoji) => format!("{emoji} {sticker_label}"),
        };
        text.chars()
            .take(TEXT_CHARS)
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect()
    }
}

/// One eligible sender and allowed body; None from content means use fixed text, not suppress posting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Content<'a> {
    /// Identity selector for the adapter's local display-name resolver.
    pub peer_id: &'a str,
    /// Content allowed by the current setting and newest media marker.
    pub preview: Preview,
}

/// Project only the newest body marker. This does not authorize posting or reveal a sender name.
/// A read sticker/photo marker still dominates retained older text; no media is fetched here.
#[must_use]
pub fn body(row: &ChatRow) -> Preview {
    if let Some(item) = &row.last_sticker {
        return if item.unread {
            Preview::Sticker(item.sticker.emoji.clone())
        } else {
            Preview::Empty
        };
    }
    let message = row
        .last_message
        .as_ref()
        .filter(|message| row.last_attachment.is_none() && message.unread);
    let text = message
        .and_then(|message| message.current.text.as_deref())
        .and_then(|text| std::str::from_utf8(text).ok());
    text.map_or(Preview::Empty, |text| {
        Preview::Text(text.chars().take(TEXT_CHARS).collect())
    })
}

/// Select allowed additions from eligible (already unmuted/unblocked) rows in core newest-first order.
/// Only the first unread increase is considered, and only an open conversation reveals additions.
/// Fixed mode, no increase, or a request/closed row returns None; the enclosing count policy decides
/// whether any fixed notification should be posted. A like/request never borrows another peer's text.
#[must_use]
pub fn content<'a>(
    mode: NoticeContent,
    before: &[ChatRow],
    after: &'a [ChatRow],
) -> Option<Content<'a>> {
    if mode == NoticeContent::Fixed {
        return None;
    }
    let row = after
        .iter()
        .find(|row| {
            let old = before.iter().find(|old| old.peer_id == row.peer_id);
            row.unread_count > old.map_or(0, |old| old.unread_count)
        })
        .filter(|row| row.state == Conversation::Open)?;
    Some(Content {
        peer_id: &row.peer_id,
        preview: if mode == NoticeContent::Text {
            body(row)
        } else {
            Preview::Empty
        },
    })
}

mod waiting;
pub use waiting::{capture, decide, Error, Notice, Waiting};

#[cfg(test)]
mod tests;
