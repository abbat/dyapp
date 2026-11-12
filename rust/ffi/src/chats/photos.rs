//! Download policy for chat originals; the core keeps strangers behind an explicit tap.

use super::{failed, ChatSession};
use crate::{network_metered, CoreError};
use dyapp_client::attachment;
use dyapp_client::store::Conversation;

/// Whole hours until default node retention ends, rounded up and shown only in the last day.
/// Zero means expired; a kept local original can still open.
#[uniffi::export]
#[must_use]
pub fn chat_photo_hours_left(sent_ms: u64, now_ms: u64) -> Option<u32> {
    attachment::hours_left(sent_ms, now_ms)
}

#[uniffi::export]
impl ChatSession {
    /// Text, photo and sticker metadata in core pair order, with `chat_history`'s text read marks.
    /// Reading history never fetches originals or reveals attachment keys.
    ///
    /// # Errors
    ///
    /// Returns an error for a blocked peer or inconsistent history projection, or if session
    /// locking, stored history or read-mark lookup fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_items(&self, peer_id: String) -> Result<Vec<super::ChatItem>, CoreError> {
        use super::ChatItem;
        use dyapp_client::store::messages::ChatItem as Stored;
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if store.blocked(&peer_id).map_err(failed)? {
            return Err(failed("blocked"));
        }
        let mut items: Vec<ChatItem> = store
            .current_chat_items(&peer_id)
            .map_err(failed)?
            .into_iter()
            .map(|item| match item {
                Stored::Sticker(sticker) => ChatItem::Sticker {
                    sticker: sticker.into(),
                },
                Stored::Text(message) => ChatItem::Text {
                    message: message.into(),
                },
                Stored::Attachment(photo) => ChatItem::Photo {
                    photo: photo.into(),
                },
            })
            .collect();
        let mut texts: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                ChatItem::Text { message } => Some(message.clone()),
                ChatItem::Photo { .. } | ChatItem::Sticker { .. } => None,
            })
            .collect();
        super::reads::project_reads(store, &peer_id, &self.key.identity.peer_id(), &mut texts)?;
        let mut texts = texts.into_iter();
        for item in &mut items {
            if let ChatItem::Text { message } = item {
                *message = texts
                    .next()
                    .ok_or_else(|| failed("history projection mismatch"))?;
            }
        }
        Ok(items)
    }

    /// Safe metadata in core history order, without downloading any original.
    /// A blocked peer is refused just as for text history.
    ///
    /// # Errors
    ///
    /// Returns an error for a blocked peer, or if the session lock or attachment-history lookup fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_photos(&self, peer_id: String) -> Result<Vec<super::ChatPhoto>, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if store.blocked(&peer_id).map_err(failed)? {
            return Err(failed("blocked"));
        }
        Ok(store
            .current_chat_attachments(&peer_id)
            .map_err(failed)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Whether originals in this visible chat may download without a tap right now.
    /// Unknown networks are metered; even Always never automatically fetches a stranger's file.
    ///
    /// # Errors
    ///
    /// Returns an error for a blocked peer, or if the session lock, download policy or
    /// conversation-state lookup fails.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_photo_auto(&self, peer_id: String) -> Result<bool, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if store.blocked(&peer_id).map_err(failed)? {
            return Err(failed("blocked"));
        }
        let mode = store.auto_download().map_err(failed)?;
        let contact = store.conversation(&peer_id).map_err(failed)? == Conversation::Open;
        Ok(attachment::auto_download(mode, network_metered(), contact))
    }
}
