//! Original bytes are looked up by stored message identity, never by a caller-supplied descriptor.

#[cfg(test)]
mod tests;

use super::{failed, ChatSession};
use crate::peer_photos::{acceptable, reachable};
use crate::profile_network::{active_network, on_network_stack};
use crate::CoreError;
use dyapp_client::attachment;
use dyapp_client::store::originals;
use dyapp_client::Error;
use dyapp_p2p_net::proto::Status;

fn fetch_error(error: Error) -> CoreError {
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
        on_network_stack(|| {
            // Keep clear/block serialized with retrieval and cache writes.
            let session = self.session.lock().map_err(failed)?;
            let store = session.store();
            if store.blocked(&peer_id).map_err(failed)? {
                return Err(failed("blocked"));
            }
            let photo = store
                .current_chat_attachments(&peer_id)
                .map_err(failed)?
                .into_iter()
                .find(|photo| {
                    photo.key.group == group_id
                        && photo.key.author == author
                        && photo.key.id.as_slice() == id
                })
                .ok_or_else(|| failed("notFound"))?
                .message
                .attachment;
            if let Some(bytes) = originals::read(&self.dir, &self.seal_key, &photo) {
                return if acceptable(&bytes, true) {
                    Ok(bytes)
                } else {
                    Err(failed("corrupted"))
                };
            }
            let network = active_network(&self.dir)?;
            let bytes = self.runtime.block_on(async {
                let mut nodes = reachable(&network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                attachment::fetch(&mut nodes, &photo)
                    .await
                    .map_err(fetch_error)
            })?;
            if !acceptable(&bytes, true) {
                return Err(failed("corrupted"));
            }
            if store.kept_originals_mib().is_ok_and(|limit| limit > 0) {
                // As on desktop, cache failure must not discard a successfully fetched image.
                if originals::keep(&self.dir, &self.seal_key, &photo, &bytes).is_ok() {
                    let _ = store.sweep_originals(&self.dir);
                }
            }
            Ok(bytes)
        })
    }
}
