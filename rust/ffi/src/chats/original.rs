//! Original bytes are looked up by stored message identity, never by a caller-supplied descriptor.

#[cfg(test)]
mod tests;

use super::{failed, ChatSession};
use crate::peer_photos::reachable_at;
use crate::profile_network::{active_network, on_network_stack};
use crate::CoreError;
use dyapp_client::attachment;
use dyapp_client::Error;
use dyapp_p2p_net::proto::Status;

pub(super) fn fetch_error(error: Error) -> CoreError {
    match error {
        Error::Status(Status::NotFound) => failed("notFound"),
        Error::Attachment(_) => failed("corrupted"),
        error => failed(error),
    }
}

#[uniffi::export]
impl ChatSession {
    /// Fetches a stored photo after an explicit tap (or a positive `chat_photo_auto` check).
    /// Errors include notFound, corrupted and notConnected; never releases the sender's blob.
    /// Blocks off the UI thread; group, author and logical ID must all match the stored message.
    ///
    /// # Errors
    ///
    /// Returns an error for a blocked peer, missing message or original, corrupt image or an
    /// unavailable network, or if worker startup, session locking, storage access or fetching fails.
    /// Failure to retain a successfully fetched image in the local cache is ignored.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_photo_original(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        self.read_chat_photo(&peer_id, &group_id, &author, &id, false)
    }

    /// Loads an original only when the current automatic download policy allows it.
    ///
    /// # Errors
    /// Returns onTap when automatic download is disabled, or the usual retrieval errors.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_photo_automatic(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
    ) -> Result<Vec<u8>, CoreError> {
        self.read_chat_photo(&peer_id, &group_id, &author, &id, true)
    }
}

impl ChatSession {
    fn read_chat_photo(
        &self,
        peer_id: &str,
        group_id: &[u8],
        author: &str,
        id: &[u8],
        automatic: bool,
    ) -> Result<Vec<u8>, CoreError> {
        on_network_stack(|| {
            // Keep clear/block serialized with retrieval and cache writes.
            let session = self.session.lock().map_err(failed)?;
            let store = session.store();
            let photo = attachment::original::select(store, peer_id, group_id, author, id)
                .map_err(failed)?;
            if automatic && !super::photos::can_auto_download(store, peer_id)? {
                return Err(failed("onTap"));
            }
            if let Some(bytes) = photo
                .local(store, &self.dir, &self.seal_key)
                .map_err(failed)?
            {
                return Ok(bytes);
            }
            let network = active_network(&self.dir)?;
            let scores = self.contact_context.lock().map_err(failed)?.scores();
            let mut scores = self.runtime.block_on(scores.lock());
            let bytes = self.runtime.block_on(async {
                let mut nodes = reachable_at(&self.dir, &network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                attachment::fetch_with_scores(&mut nodes, photo.attachment(), &mut scores)
                    .await
                    .map_err(fetch_error)
            })?;
            photo
                .finish(store, &self.dir, &self.seal_key, &bytes)
                .map_err(failed)?;
            Ok(bytes)
        })
    }
}
