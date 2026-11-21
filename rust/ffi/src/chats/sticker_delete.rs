//! Exact own-sticker deletion delegates selection and commitment to the shared client core.
use super::{ChatSession, ChatSubmission};
use crate::CoreError;
use dyapp_client::message_actions::{Kind, Source};

#[uniffi::export]
impl ChatSession {
    /// Delete one current own sticker selected by its full displayed key.
    /// Call off the UI thread; admitted identity/session ownership stays held through native completion.
    /// `Accepted`/`Queued` are committed outcomes and must never be repeated after a display failure.
    ///
    /// # Errors
    /// Returns `ownPeer`/`blocked`/`unknownMessage` for an inadmissible target, including foreign authors,
    /// malformed IDs, cleared history and non-sticker selections. Network/MLS/staging errors precede
    /// commitment; later delivery errors remain typed `Queued` outcomes.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn submit_delete_sticker(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        message_id: Vec<u8>,
    ) -> Result<ChatSubmission, CoreError> {
        self.operate_selected(
            Source {
                peer: &peer_id,
                group: &group_id,
                author: &author,
                id: &message_id,
            },
            Kind::Sticker,
            None,
        )
    }
}
