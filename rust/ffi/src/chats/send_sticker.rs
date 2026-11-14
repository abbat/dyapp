//! Saved sticker selections delegate authentication, upload policy and commitment to core.
use super::{failed, listed_own, ChatSession, ChatSubmission};
use crate::peer_photos::reachable;
use crate::profile_network::{active_network, on_network_stack};
use crate::CoreError;

#[uniffi::export]
impl ChatSession {
    /// Submit one entry from an authenticated identity-local saved pack.
    /// Call off the UI thread. A queued result is committed and must not be submitted again.
    /// Upload failures can retain an upload reservation without committing a chat message.
    ///
    /// # Errors
    /// Returns invalidPackId, invalidIndex, ownPeer or notFound for invalid selections;
    /// core pack authentication, publication, network, upload and pre-commit errors otherwise.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn submit_saved_sticker(
        &self,
        peer_id: String,
        pack_id: Vec<u8>,
        index: u32,
    ) -> Result<ChatSubmission, CoreError> {
        let pack_id: [u8; 32] = pack_id.try_into().map_err(|_| failed("invalidPackId"))?;
        let index = usize::try_from(index).map_err(|_| failed("invalidIndex"))?;
        if peer_id == self.key.identity.peer_id() {
            return Err(failed("ownPeer"));
        }
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut dyapp_client::session::Session = &mut guard;
        let pack = session
            .store()
            .saved_sticker_pack(&self.seal_key, &pack_id)
            .map_err(failed)?
            .ok_or_else(|| failed("notFound"))?;
        // Only transport selection bounds live here; the authenticated core pack owns its entries.
        if pack.stickers().get(index).is_none() {
            return Err(failed("invalidIndex"));
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
                    .submit_pack_sticker_to(
                        &mut nodes,
                        &self.key.identity,
                        &peer_id,
                        &pack,
                        index,
                        &self.seal_key,
                    )
                    .await
                    .map(|outcome| {
                        ChatSubmission::from_core(outcome, super::send_photo::send_error)
                    })
                    .map_err(super::send_photo::send_error)
            })
        })
    }
}
