//! Cache filesystem work retains the viewed identity lease and runs off the UI thread.
use crate::{chats, IdentityState};
use dyapp_client::store::originals::MediaCacheConfig;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheView {
    enabled: bool,
    limit_bytes: u64,
    used_bytes: u64,
}

enum Operation {
    Read,
    Set(MediaCacheConfig),
    Clear,
}

async fn run(
    state: &IdentityState,
    identity_id: String,
    operation: Operation,
) -> Result<CacheView, String> {
    let lease = state.lease()?;
    if lease.active().0.peer_id() != identity_id {
        return Err("Identity changed".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let store = chats::open(dir, identity)?;
        // Refuse redirected/unknown cache entries before changing retention or removing files.
        store
            .media_cache_usage(dir)
            .map_err(|error| error.to_string())?;
        match operation {
            Operation::Read => {}
            Operation::Set(config) => {
                store
                    .set_media_cache_config(config)
                    .map_err(|error| error.to_string())?;
                store
                    .sweep_originals(dir)
                    .map_err(|error| error.to_string())?;
            }
            Operation::Clear => store
                .clear_media_cache(dir)
                .map_err(|error| error.to_string())?,
        }
        let config = store
            .media_cache_config()
            .map_err(|error| error.to_string())?;
        Ok(CacheView {
            enabled: config.enabled,
            limit_bytes: config.limit_bytes,
            used_bytes: store
                .media_cache_usage(dir)
                .map_err(|error| error.to_string())?,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The caller names the identity that owns its settings screen.
#[tauri::command]
pub(crate) async fn media_cache(
    state: tauri::State<'_, IdentityState>,
    identity_id: String,
) -> Result<CacheView, String> {
    run(&state, identity_id, Operation::Read).await
}

#[tauri::command]
pub(crate) async fn set_media_cache(
    state: tauri::State<'_, IdentityState>,
    identity_id: String,
    enabled: bool,
    limit_bytes: u64,
) -> Result<CacheView, String> {
    run(
        &state,
        identity_id,
        Operation::Set(MediaCacheConfig {
            enabled,
            limit_bytes,
        }),
    )
    .await
}

#[tauri::command]
pub(crate) async fn clear_media_cache(
    state: tauri::State<'_, IdentityState>,
    identity_id: String,
) -> Result<CacheView, String> {
    run(&state, identity_id, Operation::Clear).await
}

#[cfg(test)]
mod tests;
