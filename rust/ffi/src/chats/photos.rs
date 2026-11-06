//! Download policy for chat originals; the core keeps strangers behind an explicit tap.

use super::{failed, ChatSession};
use crate::{network_metered, CoreError};
use dyapp_client::attachment;
use dyapp_client::store::Conversation;

#[uniffi::export]
impl ChatSession {
    /// Safe metadata in core history order, without downloading any original.
    /// A blocked peer is refused just as for text history.
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
