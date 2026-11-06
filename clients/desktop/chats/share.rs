//! Profile links in chats: Share sends one as an ordinary text, and a card opens the profile it
//! names; see the shared profile section of docs/design/v0.0.1-client-screens.md.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::store::Conversation;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::profile_link::profile_link;
use dyapp_identity::Identity;

use crate::profile;
use crate::search::{Found, ProfileCache, SearchState};

/// The verified profile behind a shared profile link, cached so it can be liked or messaged.
/// Errors are `invalidPeer`, `notConnected`, `unpublished` or a node message.
async fn fetch(cache: &Mutex<ProfileCache>, dir: &Path, peer_id: &str) -> Result<Found, String> {
    profile_link(peer_id).ok_or("invalidPeer")?;
    let epoch = cache.lock().map_err(|e| e.to_string())?.epoch();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let fetched = node.get_profile(peer_id).await.map_err(|e| e.to_string())?;
    if fetched.profile.deleted || fetched.profile.version == 0 {
        return Err("unpublished".into());
    }
    let found = Found::from(&fetched);
    // An older, equal or previous-network result is not cached.
    cache
        .lock()
        .map_err(|e| e.to_string())?
        .accept_if_current(epoch, fetched);
    Ok(found)
}

/// Sends `peer_id`'s profile link as an ordinary text into the open chat with `target_peer`.
async fn send(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    target_peer: &str,
) -> Result<[u8; 16], String> {
    let text = profile_link(peer_id).ok_or("invalidPeer")?;
    if peer_id == target_peer
        || super::open(dir, identity)?
            .conversation(target_peer)
            .map_err(|e| e.to_string())?
            != Conversation::Open
    {
        return Err("invalidTarget".into());
    }
    super::send_text(cache, root, dir, identity, device_key, target_peer, &text).await
}

/// The profile a shared link names; see `fetch`.
#[tauri::command]
pub(crate) async fn shared_profile(
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<Found, String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    let cache = Arc::clone(&searching.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(fetch(&cache, &dir, &peer_id))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shares a profile into an open chat; see `send`.
#[tauri::command]
pub(crate) async fn share_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    sessions: tauri::State<'_, super::Sessions>,
    peer_id: String,
    target_peer_id: String,
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let cache = Arc::clone(&searching.0);
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(send(
            &cache,
            &root,
            &dir,
            &identity,
            &device,
            &peer_id,
            &target_peer_id,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
