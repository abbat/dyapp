//! Local cache controls share the session lock with media retrieval and history cleanup.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::store::{originals::MediaCacheConfig, Store};
use std::path::Path;

/// This device's encrypted chat-media cache, separate from saved sticker packs.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ChatMediaCache {
    /// Whether opened originals are retained.
    pub enabled: bool,
    /// Remembered budget in bytes, including while retention is disabled.
    pub limit_bytes: u64,
    /// Actual encrypted original-file bytes.
    pub used_bytes: u64,
}

fn view(store: &Store, dir: &Path) -> Result<ChatMediaCache, CoreError> {
    let config = store.media_cache_config().map_err(failed)?;
    Ok(ChatMediaCache {
        enabled: config.enabled,
        limit_bytes: config.limit_bytes,
        used_bytes: store.media_cache_usage(dir).map_err(failed)?,
    })
}

#[uniffi::export]
impl ChatSession {
    /// Local settings and disk usage; no node is contacted.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock, stored cache configuration or disk-usage lookup fails.
    pub fn media_cache(&self) -> Result<ChatMediaCache, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        view(session.store(), &self.dir)
    }

    /// Applies a numeric budget or retention switch and evicts originals that no longer fit.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock, disk-usage lookup, configuration update, eviction
    /// or resulting cache-state lookup fails.
    pub fn set_media_cache(
        &self,
        enabled: bool,
        limit_bytes: u64,
    ) -> Result<ChatMediaCache, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        let store = session.store();
        store.media_cache_usage(&self.dir).map_err(failed)?;
        store
            .set_media_cache_config(MediaCacheConfig {
                enabled,
                limit_bytes,
            })
            .map_err(failed)?;
        store.sweep_originals(&self.dir).map_err(failed)?;
        view(store, &self.dir)
    }

    /// Explicit cache clear preserves history, saved packs and the remembered budget.
    ///
    /// # Errors
    ///
    /// Returns an error if the session lock, cache-file removal or resulting configuration
    /// and usage lookup fails.
    pub fn clear_media_cache(&self) -> Result<ChatMediaCache, CoreError> {
        let session = self.session.lock().map_err(failed)?;
        session
            .store()
            .clear_media_cache(&self.dir)
            .map_err(failed)?;
        view(session.store(), &self.dir)
    }
}
