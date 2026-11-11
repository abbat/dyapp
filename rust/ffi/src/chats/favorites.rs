//! Typed native adapter over the shared private-favorite policy and own-device path.
use super::ChatSession;
use crate::profile_network::on_network_stack;
use dyapp_client::favorites;

/// Stable policy errors for native favorite controls.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FavoriteError {
    /// The supplied value is not a peer ID.
    #[error("invalidPeer")]
    InvalidPeer,
    /// An identity cannot favorite itself.
    #[error("ownPeer")]
    OwnPeer,
    /// Saving a blocked peer is refused.
    #[error("blocked")]
    Blocked,
    /// Locking, storage or own-device staging failed.
    #[error("{reason}")]
    Failed {
        /// Diagnostic text; native UI must not treat it as a policy code.
        reason: String,
    },
}

fn failed(error: impl std::fmt::Display) -> FavoriteError {
    FavoriteError::Failed {
        reason: error.to_string(),
    }
}

impl From<favorites::Error> for FavoriteError {
    fn from(error: favorites::Error) -> Self {
        match error {
            favorites::Error::InvalidPeer => Self::InvalidPeer,
            favorites::Error::OwnPeer => Self::OwnPeer,
            favorites::Error::Blocked => Self::Blocked,
            error => failed(error),
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Saved peer IDs of this identity, excluding blocked peers, without network access.
    ///
    /// # Errors
    /// Returns a typed backend error if locking or reading storage fails.
    pub fn favorite_peers(&self) -> Result<Vec<String>, FavoriteError> {
        let session = self.session.lock().map_err(failed)?;
        session.store().favorite_peers().map_err(failed)
    }

    /// Read the saved flag, including blocked peers so offline removal remains possible.
    ///
    /// # Errors
    /// Returns a typed invalid-target, own-target or backend error.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn favorite_state(&self, peer_id: String) -> Result<bool, FavoriteError> {
        let session = self.session.lock().map_err(failed)?;
        favorites::state(session.store(), &peer_id).map_err(Into::into)
    }

    /// Save or remove a favorite locally and stage sync only for this identity's devices.
    ///
    /// # Errors
    /// Returns a typed target, blocked-save or backend error; removal works offline.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn set_favorite(&self, peer_id: String, saving: bool) -> Result<(), FavoriteError> {
        let mut guard = self.session.lock().map_err(failed)?;
        let session: &mut dyapp_client::session::Session = &mut guard;
        on_network_stack(|| {
            Ok(self.runtime.block_on(favorites::change(
                session,
                &mut [],
                &peer_id,
                saving,
                &self.seal_key,
            )))
        })
        .map_err(failed)?
        .map_err(Into::into)
    }
}
