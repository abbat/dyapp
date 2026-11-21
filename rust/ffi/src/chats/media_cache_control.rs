//! Thin native projection of shared exact input, targeted edits and confirmation policy.
use super::{ChatMediaCache, ChatSession};
use dyapp_client::store::originals::{
    control::{MediaCacheChange, MediaCacheChangeError, MediaCacheSnapshot},
    input, MediaCacheConfig,
};

/// Native display values derived from the actual common cache snapshot.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMediaCacheSettings {
    /// Actual retained originals state, byte budget and usage.
    pub state: ChatMediaCache,
    /// Exact MiB input for the integral-byte budget, including fractional MiB.
    pub limit_mib: String,
    /// Inclusive minimum budget in MiB, from the common core constant.
    pub min_mib: String,
    /// Inclusive maximum budget in MiB.
    pub max_mib: String,
}

impl From<MediaCacheSnapshot> for ChatMediaCacheSettings {
    fn from(snapshot: MediaCacheSnapshot) -> Self {
        Self {
            state: ChatMediaCache {
                enabled: snapshot.config.enabled,
                limit_bytes: snapshot.config.limit_bytes,
                used_bytes: snapshot.used_bytes,
            },
            limit_mib: input::mib_text(snapshot.config.limit_bytes),
            min_mib: input::mib_text(MediaCacheConfig::MIN_LIMIT_BYTES),
            max_mib: input::mib_text(MediaCacheConfig::MAX_LIMIT_BYTES),
        }
    }
}

/// One intended edit; the other actual preference is preserved by common core.
#[derive(Clone, Debug, uniffi::Enum)]
pub enum ChatMediaCacheEdit {
    /// Toggle retention without resubmitting a stale budget.
    Retention {
        /// Requested retention state.
        enabled: bool,
    },
    /// Parse a MiB input without resubmitting a stale retention state.
    Budget {
        /// Raw displayed input, validated only in common core.
        mib: String,
    },
    /// Explicitly clear cached originals.
    Clear,
}

impl From<ChatMediaCacheEdit> for MediaCacheChange {
    fn from(edit: ChatMediaCacheEdit) -> Self {
        match edit {
            ChatMediaCacheEdit::Retention { enabled } => Self::Retention(enabled),
            ChatMediaCacheEdit::Budget { mib } => Self::Budget(mib),
            ChatMediaCacheEdit::Clear => Self::Clear,
        }
    }
}

/// A measured result or a no-mutation confirmation request.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMediaCacheResult {
    /// Current measured preferences and usage.
    pub settings: ChatMediaCacheSettings,
    /// The requested destructive operation has not yet run.
    pub confirmation_required: bool,
}

/// Stable validation refusal stays distinct from an indeterminate backend failure.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ChatMediaCacheError {
    /// Invalid MiB text, exact-byte conversion, or supported range.
    #[error("invalidLimit")]
    InvalidLimit,
    /// Diagnostic failure; a write may already have committed.
    #[error("{reason}")]
    Failed {
        /// Diagnostic text, not a stable policy code.
        reason: String,
    },
}

fn failed(error: impl std::fmt::Display) -> ChatMediaCacheError {
    ChatMediaCacheError::Failed {
        reason: error.to_string(),
    }
}

impl From<MediaCacheChangeError> for ChatMediaCacheError {
    fn from(error: MediaCacheChangeError) -> Self {
        match error {
            MediaCacheChangeError::InvalidLimit => Self::InvalidLimit,
            error => failed(error),
        }
    }
}

#[uniffi::export]
impl ChatSession {
    /// Local display and exact input values, under the ordinary media session lock.
    ///
    /// # Errors
    /// Returns a typed backend failure instead of an empty/default cache.
    pub fn media_cache_settings(&self) -> Result<ChatMediaCacheSettings, ChatMediaCacheError> {
        let session = self.session.lock().map_err(failed)?;
        session
            .store()
            .media_cache_snapshot(&self.dir)
            .map(Into::into)
            .map_err(Into::into)
    }

    /// Reevaluate a targeted edit against actual core state and request confirmation if needed.
    ///
    /// # Errors
    /// Returns typed invalid input or indeterminate storage/filesystem failure. Keep failed edits
    /// read-only until successful readback; this method contacts no node.
    pub fn change_media_cache(
        &self,
        edit: ChatMediaCacheEdit,
        confirmed: bool,
    ) -> Result<ChatMediaCacheResult, ChatMediaCacheError> {
        let session = self.session.lock().map_err(failed)?;
        let result = session
            .store()
            .change_media_cache(&self.dir, &edit.into(), confirmed)?;
        Ok(ChatMediaCacheResult {
            settings: result.state.into(),
            confirmation_required: result.confirmation_required,
        })
    }
}
