//! Persisted per-identity download policy shared by chat and profile photos.

use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::attachment::AutoDownload;

/// When accepted contacts' full photos may download automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ChatPhotoDownload {
    /// Only on a reported unmetered network.
    ByNetwork,
    /// On any network; strangers still require a tap.
    Always,
    /// Only after an explicit tap.
    OnTap,
}

impl From<ChatPhotoDownload> for AutoDownload {
    fn from(mode: ChatPhotoDownload) -> Self {
        match mode {
            ChatPhotoDownload::ByNetwork => Self::ByNetwork,
            ChatPhotoDownload::Always => Self::Always,
            ChatPhotoDownload::OnTap => Self::OnTap,
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// This identity's persisted download choice on this device.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or stored download-policy lookup fails.
    pub fn photo_download(&self) -> Result<ChatPhotoDownload, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        Ok(match session.store().auto_download().map_err(failed)? {
            AutoDownload::ByNetwork => ChatPhotoDownload::ByNetwork,
            AutoDownload::Always => ChatPhotoDownload::Always,
            AutoDownload::OnTap => ChatPhotoDownload::OnTap,
        })
    }

    /// Changes the shared chat/profile original download policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock or storing the download policy fails.
    pub fn set_photo_download(&self, mode: ChatPhotoDownload) -> Result<(), CoreError> {
        self.session
            .lock()
            .map_err(failed)?
            .store()
            .set_auto_download(mode.into())
            .map_err(failed)
    }
}
