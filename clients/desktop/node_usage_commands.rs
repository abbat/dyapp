//! Active-slot node and storage-status commands.

#[cfg(unix)]
use crate::node;
use crate::{config_dir, usage, IdentityState};
#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use tauri::Manager;

/// The open identity's node settings; see `node::load`.
#[cfg(unix)]
#[tauri::command]
pub(super) async fn load_node_settings(
    state: tauri::State<'_, IdentityState>,
) -> Result<node::Settings, String> {
    let lease = state.lease()?;
    node::load(&lease.active().1)
}

/// Saves node settings and restarts the node as they say; see `node::apply`.
#[cfg(unix)]
#[tauri::command]
pub(super) async fn save_node_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    settings: node::Settings,
) -> Result<HashMap<&'static str, String>, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    // apply spawns key generation; keep the lease when the UI command is cancelled.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        node::save(&dir, &settings)?;
        tauri::async_runtime::block_on(node::apply(&app.state::<node::NodeState>(), &dir))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Starts or stops the node for the selected identity.
#[cfg(unix)]
#[tauri::command]
pub(super) async fn apply_node(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<HashMap<&'static str, String>, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(node::apply(&app.state::<node::NodeState>(), &dir))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(unix)]
#[tauri::command]
pub(super) async fn node_status(
    state: tauri::State<'_, IdentityState>,
    nodes: tauri::State<'_, node::NodeState>,
) -> Result<HashMap<&'static str, String>, String> {
    let lease = state.lease()?;
    Ok(node::status(&nodes, &lease.active().1).await)
}

/// Settings → Storage for the selected identity; see `usage::usage`.
#[tauri::command]
pub(super) async fn storage_usage(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<usage::Usage, String> {
    let config = config_dir(&app).ok();
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    // Cancellation of the caller does not stop this blocking filesystem scan.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        usage::usage(config.as_deref(), &dir)
    })
    .await
    .map_err(|error| error.to_string())
}

/// Status-bar traffic and the selected identity's message-store bytes.
#[tauri::command]
pub(super) fn status(state: tauri::State<'_, IdentityState>) -> usage::Status {
    usage::Status {
        traffic: dyapp_client::traffic(),
        messages: state
            .lease()
            .ok()
            .and_then(|lease| usage::messages(&lease.active().1)),
    }
}
