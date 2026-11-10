//! Send one prepared JPEG into an already open conversation through the shared Session.

#[cfg(test)]
mod tests;

use super::{failed, listed_own, ChatSession};
use crate::peer_photos::reachable;
use crate::profile_network::{active_network, on_network_stack};
use crate::CoreError;
use dyapp_client::attachment::{self, Attachment};
use dyapp_client::photo::prepare::{validate_original, Purpose};
use dyapp_client::session;
use dyapp_client::store::Conversation;
use dyapp_client::Error;
use dyapp_p2p_net::proto::Status;

fn sealed(
    photo: &[u8],
    preview: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(Attachment, Vec<u8>), CoreError> {
    if !photo.starts_with(&[0xff, 0xd8]) {
        return Err(failed("unsupported"));
    }
    validate_original(photo, width, height, Purpose::Chat).map_err(|_| failed("corrupted"))?;
    if !preview.is_empty() {
        crate::check_preview(preview.clone())?;
    }
    let (mut media, blob) = attachment::seal(photo).map_err(failed)?;
    media.preview = preview;
    media.mime = "image/jpeg".into();
    media.width = width;
    media.height = height;
    media.check().map_err(failed)?;
    Ok((media, blob))
}

fn send_error(error: session::Error) -> CoreError {
    match error {
        session::Error::Client(Error::Status(Status::Full)) => failed("full"),
        session::Error::Client(Error::Status(Status::TooLarge)) => failed("tooLarge"),
        session::Error::Client(Error::Status(Status::NotFound)) => failed("peerUnavailable"),
        error => failed(error),
    }
}

#[uniffi::export]
impl ChatSession {
    /// Sends a bounded prepared JPEG and its preview to an open chat, never a message request.
    /// Nothing is staged if upload fails; a later delivery failure may leave a stored photo.
    /// Errors include unsupported, corrupted, blocked, conversationNotOpen, full and tooLarge.
    ///
    /// # Errors
    ///
    /// Returns an error for the own or blocked peer, a conversation that is not open, invalid
    /// image data or an unpublished device, or if session locking, encryption, storage or network
    /// access fails. Node failures include `full`, `tooLarge` and `peerUnavailable`.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn send_chat_photo(
        &self,
        peer_id: String,
        photo: Vec<u8>,
        preview: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, CoreError> {
        if peer_id == self.key.identity.peer_id() {
            return Err(failed("ownPeer"));
        }
        let (media, blob) = sealed(&photo, preview, width, height)?;
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut session::Session = &mut guard;
        if session.store().blocked(&peer_id).map_err(failed)? {
            return Err(failed("blocked"));
        }
        if session.store().conversation(&peer_id).map_err(failed)? != Conversation::Open {
            return Err(failed("conversationNotOpen"));
        }
        listed_own(session, &self.key.identity)?;
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut nodes = reachable(&network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                session
                    .send_attachment_to(
                        &mut nodes,
                        &self.key.identity,
                        &peer_id,
                        &media,
                        &blob,
                        &self.seal_key,
                    )
                    .await
                    .map(|id| id.to_vec())
                    .map_err(send_error)
            })
        })
    }
}
