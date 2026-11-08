//! What a local notice shows.

use super::{Error, Store};

/// What a local notice shows on a screen others may see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoticeContent {
    /// Only the fixed text.
    #[default]
    Fixed,
    /// The sender's name.
    Name,
    /// The sender's name and the message.
    Text,
}

impl Store {
    /// What a local notice shows.
    ///
    /// # Errors
    ///
    /// Returns an error if the setting cannot be read or its encoding or value is invalid.
    pub fn notice_content(&self) -> Result<NoticeContent, Error> {
        match self.setting_byte("notice_content")? {
            None | Some(0) => Ok(NoticeContent::Fixed),
            Some(1) => Ok(NoticeContent::Name),
            Some(2) => Ok(NoticeContent::Text),
            _ => Err(Error::InvalidSetting),
        }
    }

    /// Persist what a local notice shows.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the notice setting to the database fails.
    pub fn set_notice_content(&self, content: NoticeContent) -> Result<(), Error> {
        let value = match content {
            NoticeContent::Fixed => 0,
            NoticeContent::Name => 1,
            NoticeContent::Text => 2,
        };
        self.set_setting_byte("notice_content", value)
    }
}
