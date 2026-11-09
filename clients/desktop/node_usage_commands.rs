//! Installation node and active-profile storage/status commands.

use crate::node;
use crate::{config_dir, usage, IdentityState};
use dyapp_identity::network::{self, Settings as NetworkFile};
use std::collections::HashMap;
use tauri::Manager;

fn selected_network(state: &IdentityState) -> Result<Option<network::Network>, String> {
    let Ok(lease) = state.lease() else {
        return Ok(None);
    };
    network::load(&lease.active().1, NetworkFile::Active).map_err(|error| error.to_string())
}

/// Installation settings remain available without an open identity.
#[tauri::command]
pub(super) async fn load_node_settings(app: tauri::AppHandle) -> Result<node::Settings, String> {
    node::load(&config_dir(&app)?)
}

/// Node work owns no identity lease after the active-network snapshot is read.
#[tauri::command]
pub(super) async fn save_node_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    settings: node::Settings,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let active = if settings.enabled {
        selected_network(&state)?
    } else {
        None
    };
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(node::configure(
            &app.state::<node::NodeState>(),
            &dir,
            Some(settings),
            active,
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Explicit activation adopts the selected active network; identity switches never call this.
#[tauri::command]
pub(super) async fn apply_node(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
) -> Result<HashMap<&'static str, String>, String> {
    let dir = config_dir(&app)?;
    let active = selected_network(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        tauri::async_runtime::block_on(node::configure(
            &app.state::<node::NodeState>(),
            &dir,
            None,
            active,
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(super) async fn node_status(
    app: tauri::AppHandle,
    nodes: tauri::State<'_, node::NodeState>,
) -> Result<HashMap<&'static str, String>, String> {
    Ok(node::status(&nodes, &config_dir(&app)?).await)
}

/// Restore the installation node before any account is opened.
pub(super) fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let result = match config_dir(&app) {
            Ok(dir) => node::apply(&app.state::<node::NodeState>(), &dir)
                .await
                .map(|_| ()),
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            eprintln!("Embedded node startup failed: {error}");
        }
    });
}

/// Wait for the node stores to close on application exit, including explicit Quit.
pub(super) fn shutdown(app: &tauri::AppHandle) {
    tauri::async_runtime::block_on(node::stop(&app.state::<node::NodeState>()));
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
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(super) fn status(state: tauri::State<'_, IdentityState>) -> usage::Status {
    usage::Status {
        traffic: dyapp_client::traffic(),
        messages: state
            .lease()
            .ok()
            .and_then(|lease| usage::messages(&lease.active().1)),
    }
}
