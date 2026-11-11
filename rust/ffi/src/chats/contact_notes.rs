//! Typed native private-note adapter over common core policy and own-device staging.
use super::ChatSession;
use crate::profile_network::on_network_stack;
use dyapp_client::contact_notes;

/// Stable private-note failures; a backend error is never an empty note.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ContactNoteError {
    /// The supplied value is not a peer ID.
    #[error("invalidPeer")]
    InvalidPeer,
    /// An identity cannot annotate itself as a contact.
    #[error("ownPeer")]
    OwnPeer,
    /// The text violates the shared note limit or clean-text rules.
    #[error("invalidText")]
    InvalidText,
    /// Locking, storage or own-device staging failed.
    #[error("{reason}")]
    Failed {
        /// Diagnostic text, not a stable policy code.
        reason: String,
    },
}

fn failed(error: impl std::fmt::Display) -> ContactNoteError {
    ContactNoteError::Failed {
        reason: error.to_string(),
    }
}

impl From<contact_notes::Error> for ContactNoteError {
    fn from(error: contact_notes::Error) -> Self {
        match error {
            contact_notes::Error::InvalidPeer => Self::InvalidPeer,
            contact_notes::Error::OwnPeer => Self::OwnPeer,
            contact_notes::Error::InvalidText => Self::InvalidText,
            error => failed(error),
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Read local private text, including for blocked contacts, without network access.
    ///
    /// # Errors
    /// Returns typed target or backend errors; only a successful missing note is empty.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn contact_note(&self, peer_id: String) -> Result<String, ContactNoteError> {
        let session = self.session.lock().map_err(failed)?;
        contact_notes::read(session.store(), &peer_id).map_err(Into::into)
    }

    /// Save exact clean text or clear with empty text, staging only own-device sync.
    ///
    /// # Errors
    /// Returns typed target, text or backend errors; no profile fetch or peer signal occurs.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn set_contact_note(&self, peer_id: String, note: String) -> Result<(), ContactNoteError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut dyapp_client::session::Session = &mut guard;
        on_network_stack(|| {
            Ok(self.runtime.block_on(contact_notes::change(
                session,
                &mut [],
                &peer_id,
                note,
                &self.seal_key,
            )))
        })
        .map_err(failed)?
        .map_err(Into::into)
    }
}
