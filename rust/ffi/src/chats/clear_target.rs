//! Read-only Clear presentation uses the same common visibility policy as native actions.
use super::{failed, ChatSession};
use crate::CoreError;

/// Identifiers projected under the native session lease; not a Clear authorization.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatClearTarget {
    /// Owner of the admitted native store.
    pub owner_peer_id: String,
    /// Exact known visible conversation peer.
    pub peer_id: String,
}

#[uniffi::export]
impl ChatSession {
    /// Read the target used by a Clear warning without changing history or sending anything.
    ///
    /// # Errors
    /// Returns existing ownPeer/blocked/unknownPeer refusals, session-lock or storage errors.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned native strings"
    )]
    pub fn clear_target(&self, peer_id: String) -> Result<ChatClearTarget, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let target =
            dyapp_client::chat_access::target(session.store(), &peer_id).map_err(failed)?;
        Ok(ChatClearTarget {
            owner_peer_id: target.owner_peer_id,
            peer_id: target.peer_id,
        })
    }
}
