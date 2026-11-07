//! Send a selected validated saved entry through the same gated Session as chat photos.
use super::owner_matches;
use crate::{
    chats,
    search::{ProfileCache, SearchState},
    IdentityState,
};
use dyapp_identity::Identity;
use serde::Deserialize;
use std::{path::Path, sync::Mutex};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    peer_id: String,
    pack_id: [u8; 32],
    index: usize,
}
async fn send(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device: &Identity,
    selection: &Selection,
) -> Result<[u8; 16], String> {
    if selection.peer_id == identity.peer_id() {
        return Err("invalidTarget".into());
    }
    let store = chats::open(dir, identity)?;
    let key = crate::sealing_key(root, dir, &store)?;
    let pack = store
        .saved_sticker_pack(&key, &selection.pack_id)
        .map_err(|error| error.to_string())?
        .ok_or("notFound")?;
    if selection.index >= pack.stickers().len() {
        return Err("invalidIndex".into());
    }
    let (mut session, mut node, key) = chats::forward::open_session(
        cache,
        root,
        dir,
        identity,
        device,
        store,
        &selection.peer_id,
    )
    .await?;
    session
        .send_pack_sticker_to(
            std::slice::from_mut(&mut node),
            identity,
            &selection.peer_id,
            &pack,
            selection.index,
            &key,
        )
        .await
        .map_err(chats::send_photo::code)
}

/// Return the durable queued message ID; upload failures have no chat message to retry.
#[tauri::command]
pub(crate) async fn send_saved_sticker(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    selection: Selection,
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let cache = searching.inner().0.clone();
    let sessions = sessions.inner().0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _guard = sessions.lock().map_err(|_| "Session state unavailable")?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(send(&cache, &root, &dir, &identity, &device, &selection))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests;
