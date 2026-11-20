//! Targeted settings edits evaluated against the current cache under the caller's session guard.
use super::{input, Error, MediaCacheConfig, Store};
use std::{io, path::Path};

/// A single field edit, or an explicitly requested cache clear.
#[derive(Clone, Debug)]
pub enum MediaCacheChange {
    /// Preserve the actual remembered budget while changing retention.
    Retention(bool),
    /// Preserve actual retention while parsing an exact MiB budget.
    Budget(String),
    /// Preserve settings, history and saved sticker packs.
    Clear,
}

/// A current device-local cache snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaCacheSnapshot {
    /// Actual persisted retention and budget.
    pub config: MediaCacheConfig,
    /// Actual encrypted-file bytes observed on disk.
    pub used_bytes: u64,
}

/// Either an applied edit or a confirmation request with no mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaCacheChangeResult {
    /// Measured state, not a speculative UI value.
    pub state: MediaCacheSnapshot,
    /// A destructive edit has not run and needs explicit confirmation.
    pub confirmation_required: bool,
}

/// Invalid input stays distinct from an indeterminate storage/filesystem failure.
#[derive(Debug, thiserror::Error)]
pub enum MediaCacheChangeError {
    /// Not an exact integral-byte budget within the existing cache range.
    #[error("invalid media cache MiB limit")]
    InvalidLimit,
    /// Cache settings could not be read or written.
    #[error(transparent)]
    Store(#[from] Error),
    /// File validation, eviction or resulting usage failed; earlier work may have committed.
    #[error(transparent)]
    Files(#[from] io::Error),
}

impl MediaCacheChange {
    fn config(&self, current: MediaCacheConfig) -> Result<MediaCacheConfig, MediaCacheChangeError> {
        Ok(match self {
            Self::Retention(enabled) => MediaCacheConfig {
                enabled: *enabled,
                ..current
            },
            Self::Budget(text) => MediaCacheConfig {
                limit_bytes: input::parse_mib(text).ok_or(MediaCacheChangeError::InvalidLimit)?,
                ..current
            },
            Self::Clear => current,
        })
    }
}

impl Store {
    /// Read current configuration and validated disk usage without contacting a node.
    ///
    /// # Errors
    /// Returns typed store or filesystem failures, never a synthetic empty cache.
    pub fn media_cache_snapshot(
        &self,
        dir: &Path,
    ) -> Result<MediaCacheSnapshot, MediaCacheChangeError> {
        Ok(MediaCacheSnapshot {
            config: self.media_cache_config()?,
            used_bytes: self.media_cache_usage(dir)?,
        })
    }

    /// Apply one field edit against current state; refuse destructive work until confirmed.
    ///
    /// The caller retains its ordinary media/session guard through completion. A previous UI
    /// snapshot is not authority: configuration and actual usage are read again here.
    ///
    /// # Errors
    /// Returns invalid-limit, store or filesystem failure. Write/eviction/readback failure may
    /// occur after a durable change, so callers must reconcile before another mutation.
    pub fn change_media_cache(
        &self,
        dir: &Path,
        change: &MediaCacheChange,
        confirmed: bool,
    ) -> Result<MediaCacheChangeResult, MediaCacheChangeError> {
        let before = self.media_cache_snapshot(dir)?;
        let config = change.config(before.config)?;
        let destructive = matches!(change, MediaCacheChange::Clear)
            || input::requires_confirmation(before.config, config, before.used_bytes);
        if destructive && !confirmed {
            return Ok(MediaCacheChangeResult {
                state: before,
                confirmation_required: true,
            });
        }
        if let MediaCacheChange::Clear = change {
            self.clear_media_cache(dir)?;
        } else {
            self.set_media_cache_config(config)?;
            self.sweep_originals(dir)?;
        }
        Ok(MediaCacheChangeResult {
            state: self.media_cache_snapshot(dir)?,
            confirmation_required: false,
        })
    }
}

#[cfg(test)]
mod tests;
