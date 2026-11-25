//! Desktop command transport for the shared owner search recovery.
use crate::identity_runtime::IdentityState;
use crate::search_recovery_state::{Failure, RecoveryState, Saved};
use dyapp_client::store::{SearchWithdrawalRecovery, Store};
use dyapp_identity::network::{self, Settings};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[tauri::command]
pub(super) async fn prepare_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
) -> Result<Option<String>, Failure> {
    let lease = state.lease().map_err(Failure::failed)?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        reviews.prepare(identity, dir, lease.generation())
    })
    .await
    .map_err(Failure::failed)?
}

#[tauri::command]
pub(super) async fn confirm_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<Saved, Failure> {
    let lease = state.lease().map_err(Failure::failed)?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        reviews.confirm(identity, dir, lease.generation(), &token)
    })
    .await
    .map_err(Failure::failed)?
}

#[tauri::command]
pub(super) async fn retry_search_recovery(
    state: tauri::State<'_, IdentityState>,
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<Saved, Failure> {
    let lease = state.lease().map_err(Failure::failed)?;
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(Failure::failed)?
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
    .map_err(Failure::failed)?
}

#[tauri::command]
pub(super) async fn dismiss_search_recovery(
    reviews: tauri::State<'_, RecoveryState>,
    token: String,
) -> Result<(), Failure> {
    let reviews = reviews.inner().clone();
    tauri::async_runtime::spawn_blocking(move || reviews.dismiss(&token))
        .await
        .map_err(Failure::failed)?
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
    let mut entry = crate::profile::connect_at(dir, &network).await?;
    let protocols =
        Arc::new(dyapp_p2p_net::Protocols::new(&network.name).map_err(|error| error.to_string())?);
    let scores =
        dyapp_client::contact_verification::scores(dir).map_err(|error| error.to_string())?;
    let mut scores = scores.lock().await;
    let mut reached = dyapp_client::search::connect_search_nodes_with_scores(
        &mut entry,
        protocols,
        dyapp_client::publish::SEARCH_NODES,
        &mut scores,
    )
    .await;
    dyapp_client::search::publish_recovery_with_scores(
        store,
        plan,
        &mut reached.nodes,
        &mut scores,
        now,
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}
