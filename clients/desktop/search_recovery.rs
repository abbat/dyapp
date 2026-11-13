//! Desktop command transport for the shared owner search recovery.
use crate::identity_runtime::IdentityState;
use crate::search_recovery_state::{RecoveryState, Saved};
use dyapp_client::store::{SearchWithdrawalRecovery, Store};
use dyapp_identity::network::{self, Settings};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[tauri::command]
pub(super) async fn prepare_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
) -> Result<Option<String>, String> {
    let lease = state.lease()?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        reviews.prepare(identity, dir, lease.generation())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(super) async fn confirm_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<Saved, String> {
    let lease = state.lease()?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        reviews.confirm(identity, dir, lease.generation(), &token)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(super) async fn retry_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<Saved, String> {
    let lease = state.lease()?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs();
        reviews.retry(
            identity,
            dir,
            lease.generation(),
            &token,
            now,
            |store, plan| tauri::async_runtime::block_on(send(store, plan, dir, now)),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(super) async fn dismiss_search_recovery(
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<(), String> {
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || reviews.dismiss(&token))
        .await
        .map_err(|error| error.to_string())?
}

async fn send(
    store: &Store,
    plan: &SearchWithdrawalRecovery,
    dir: &Path,
    now: u64,
) -> Result<(), String> {
    let network = network::load(dir, Settings::Active)
        .map_err(|error| error.to_string())?
        .ok_or("notConnected")?;
    let mut entry = crate::profile::connect(&network).await?;
    let protocols =
        Arc::new(dyapp_p2p_net::Protocols::new(&network.name).map_err(|error| error.to_string())?);
    let mut reached = dyapp_client::search::connect_search_nodes(&mut entry, protocols, 2).await;
    dyapp_client::search::publish_recovery(store, plan, &mut reached.nodes, now)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
