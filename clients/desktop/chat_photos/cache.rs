//! Cache filesystem work retains the viewed identity lease and runs off the UI thread.
use crate::{chats, IdentityState};
use dyapp_client::store::{
    originals::{
        control::{MediaCacheChange, MediaCacheChangeError, MediaCacheSnapshot},
        input, MediaCacheConfig,
    },
    Store,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheView {
    enabled: bool,
    limit_bytes: u64,
    used_bytes: u64,
    min_bytes: u64,
    max_bytes: u64,
    limit_mib: String,
    min_mib: String,
    max_mib: String,
    confirmation_required: bool,
}

impl CacheView {
    fn new(state: MediaCacheSnapshot, confirmation_required: bool) -> Self {
        Self {
            enabled: state.config.enabled,
            limit_bytes: state.config.limit_bytes,
            used_bytes: state.used_bytes,
            min_bytes: MediaCacheConfig::MIN_LIMIT_BYTES,
            max_bytes: MediaCacheConfig::MAX_LIMIT_BYTES,
            limit_mib: input::mib_text(state.config.limit_bytes),
            min_mib: input::mib_text(MediaCacheConfig::MIN_LIMIT_BYTES),
            max_mib: input::mib_text(MediaCacheConfig::MAX_LIMIT_BYTES),
            confirmation_required,
        }
    }
}

/// Target one actual preference, never replay the other field from a stale UI snapshot.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum Change {
    Retention { enabled: bool },
    Budget { mib: String },
    Clear,
}

impl From<Change> for MediaCacheChange {
    fn from(change: Change) -> Self {
        match change {
            Change::Retention { enabled } => Self::Retention(enabled),
            Change::Budget { mib } => Self::Budget(mib),
            Change::Clear => Self::Clear,
        }
    }
}

enum Operation {
    Read,
    Change(MediaCacheChange, bool),
}

fn perform(store: &Store, dir: &Path, operation: Operation) -> Result<CacheView, String> {
    match operation {
        Operation::Read => {}
        Operation::Change(change, confirmed) => {
            let result = store
                .change_media_cache(dir, &change, confirmed)
                .map_err(|error| match error {
                    MediaCacheChangeError::InvalidLimit => "invalidLimit".into(),
                    other => other.to_string(),
                })?;
            return Ok(CacheView::new(result.state, result.confirmation_required));
        }
    }
    let snapshot = store
        .media_cache_snapshot(dir)
        .map_err(|error| error.to_string())?;
    Ok(CacheView::new(snapshot, false))
}

async fn run(
    state: &IdentityState,
    sessions: &chats::Sessions,
    identity_id: String,
    operation: Operation,
) -> Result<CacheView, String> {
    let lease = state.lease()?;
    if lease.active().0.peer_id() != identity_id {
        return Err("Identity changed".into());
    }
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _sessions = sessions.lock().map_err(|error| error.to_string())?;
        let (identity, dir) = lease.active();
        perform(&chats::open(dir, identity)?, dir, operation)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The caller names the identity that owns its settings screen.
#[tauri::command]
pub(crate) async fn media_cache(
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
) -> Result<CacheView, String> {
    run(&state, &sessions, identity_id, Operation::Read).await
}

#[tauri::command]
pub(crate) async fn change_media_cache(
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    change: Change,
    confirmed: bool,
) -> Result<CacheView, String> {
    run(
        &state,
        &sessions,
        identity_id,
        Operation::Change(change.into(), confirmed),
    )
    .await
}

#[cfg(test)]
mod tests;
