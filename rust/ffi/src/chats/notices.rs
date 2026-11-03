//! The new-mail notice setting, kept in the same Store keys as the desktop privacy screen.

use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::store::NoticeContent;

/// What a new-mail notice shows, or that none shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatNotice {
    /// No notice.
    Off,
    /// Only the fixed "New message" text.
    Fixed,
    /// The sender's name too.
    Name,
    /// The sender's name and the message text too.
    Text,
}

#[uniffi::export]
impl ChatSession {
    /// The notice setting; the default is [`ChatNotice::Fixed`].
    pub fn notice(&self) -> Result<ChatNotice, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        if !store.notifications().map_err(failed)? {
            return Ok(ChatNotice::Off);
        }
        Ok(match store.notice_content().map_err(failed)? {
            NoticeContent::Fixed => ChatNotice::Fixed,
            NoticeContent::Name => ChatNotice::Name,
            NoticeContent::Text => ChatNotice::Text,
        })
    }

    /// Store the notice setting; [`ChatNotice::Off`] keeps the chosen content for a later turn-on.
    pub fn set_notice(&self, notice: ChatNotice) -> Result<(), CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        let content = match notice {
            ChatNotice::Off => return store.set_notifications(false).map_err(failed),
            ChatNotice::Fixed => NoticeContent::Fixed,
            ChatNotice::Name => NoticeContent::Name,
            ChatNotice::Text => NoticeContent::Text,
        };
        store.set_notice_content(content).map_err(failed)?;
        store.set_notifications(true).map_err(failed)
    }
}
