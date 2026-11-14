//! Native status is a projection of shared core decisions, including saved-intent reconciliation.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::{
    sticker_requests as core,
    store::{messages::MessageKey, Store},
};

/// Authoritative received-pack save phase, independent of localized presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StickerPackSaveState {
    /// No saved file or pending intent.
    New,
    /// The authenticated saved pack contains the clicked sticker.
    Saved,
    /// Waiting for a peer descriptor.
    Waiting,
    /// An admitted descriptor is available for download.
    Downloading,
    /// A typed local failure is available.
    Failed,
}
impl From<core::State> for StickerPackSaveState {
    fn from(value: core::State) -> Self {
        match value {
            core::State::New => Self::New,
            core::State::Saved => Self::Saved,
            core::State::Waiting => Self::Waiting,
            core::State::Downloading => Self::Downloading,
            core::State::Failed => Self::Failed,
        }
    }
}

/// Display-safe core failure; no raw peer data or attachment capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StickerPackSaveFailure {
    /// Attachment transport failed.
    Fetch,
    /// Received pack validation failed.
    Invalid,
    /// Local saved-pack quota is full.
    Quota,
    /// Local saved content/storage cannot be read.
    Storage,
    /// An installed pack does not contain the clicked sticker.
    StickerMissing,
}
impl From<core::Failure> for StickerPackSaveFailure {
    fn from(value: core::Failure) -> Self {
        match value {
            core::Failure::Fetch => Self::Fetch,
            core::Failure::Invalid => Self::Invalid,
            core::Failure::Quota => Self::Quota,
            core::Failure::Storage => Self::Storage,
            core::Failure::StickerMissing => Self::StickerMissing,
        }
    }
}

/// Current identity-local request status, with no descriptors or encryption keys.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StickerPackSaveStatus {
    /// Core-computed phase.
    pub state: StickerPackSaveState,
    /// Current 16-byte intent generation, when present.
    pub generation: Option<Vec<u8>>,
    /// Typed failure when failed.
    pub failure: Option<StickerPackSaveFailure>,
    /// Exact core Unix-millisecond retry deadline, when a descriptor exists.
    pub retry_at_ms: Option<u64>,
}
impl From<core::Status> for StickerPackSaveStatus {
    fn from(value: core::Status) -> Self {
        Self {
            state: value.state.into(),
            generation: value.generation.map(|token| token.to_vec()),
            failure: value.failure.map(Into::into),
            retry_at_ms: value.retry_at_ms,
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Read current received-pack status by its complete stored message key, without networking.
    /// This can reconcile an already-saved intent; call off the UI thread under the session lease.
    /// A failed read never means New or Saved.
    ///
    /// # Errors
    /// Returns malformed/current-message, own-authored source, locking, storage or reconciliation
    /// failures. Unreadable pack content remains explicit Failed/Storage with its current generation.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn chat_sticker_pack_status(
        &self,
        peer_id: String,
        group_id: Vec<u8>,
        author: String,
        id: Vec<u8>,
    ) -> Result<StickerPackSaveStatus, CoreError> {
        let key = MessageKey {
            group: group_id,
            author,
            id: id.try_into().map_err(|_| failed("unknownMessage"))?,
        };
        let _guard = self.session.lock().map_err(failed)?;
        let mut store =
            Store::open(&self.dir.join("client.db"), &self.key.identity).map_err(failed)?;
        core::status(&mut store, &self.seal_key, &peer_id, &key)
            .map(Into::into)
            .map_err(failed)
    }
}
