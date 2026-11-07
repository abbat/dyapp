//! A screen names a current message; media capabilities and peer WebP stay in native code.
use super::owner_matches;
use crate::{chat_photos, chats, profile, IdentityState};
use dyapp_client::{
    attachment,
    sticker_webp::decode::Frame,
    store::{
        messages::{ChatSticker, MessageKey},
        originals, Conversation, Store,
    },
};
use dyapp_identity::{
    network::{self, Settings},
    Identity,
};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Selection {
    pub peer_id: String,
    pub group_id: Vec<u8>,
    pub author: String,
    pub id: [u8; 16],
}
impl Selection {
    pub(crate) fn key(&self) -> MessageKey {
        MessageKey {
            group: self.group_id.clone(),
            author: self.author.clone(),
            id: self.id,
        }
    }
}
fn selected_in(items: Vec<ChatSticker>, selected: &Selection) -> Result<ChatSticker, String> {
    let key = selected.key();
    items
        .into_iter()
        .find(|item| item.key == key)
        .ok_or_else(|| "unknownMessage".into())
}
pub(crate) fn selected(store: &Store, selection: &Selection) -> Result<ChatSticker, String> {
    if selection.peer_id == store.owner_peer_id() {
        return Err("ownPeer".into());
    }
    if store
        .blocked(&selection.peer_id)
        .map_err(|e| e.to_string())?
    {
        return Err("blocked".into());
    }
    if store
        .conversation(&selection.peer_id)
        .map_err(|e| e.to_string())?
        == Conversation::None
    {
        return Err("unknownPeer".into());
    }
    selected_in(
        store
            .current_chat_stickers(&selection.peer_id)
            .map_err(|e| e.to_string())?,
        selection,
    )
}
pub(crate) fn frame_bytes(frame: Frame) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + frame.pixels.len());
    bytes.extend(frame.width.to_le_bytes());
    bytes.extend(frame.height.to_le_bytes());
    bytes.extend(frame.pixels);
    bytes
}
fn current(
    dir: &Path,
    identity: &Identity,
    selection: &Selection,
) -> Result<(Store, ChatSticker), String> {
    let store = chats::open(dir, identity)?;
    let item = selected(&store, selection)?;
    Ok((store, item))
}
fn retain(
    dir: &Path,
    identity: &Identity,
    selection: &Selection,
    item: &ChatSticker,
    key: &[u8; 32],
    bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let (store, latest) = current(dir, identity, selection)?;
    if latest.sticker != item.sticker {
        return Err("unknownMessage".into());
    }
    let frame = latest.sticker.first_frame(bytes).map_err(|_| "corrupted")?;
    if let Some(descriptor) = latest.sticker.download() {
        if store
            .media_cache_config()
            .map_err(|e| e.to_string())?
            .enabled
        {
            // As with photos, failed retention must not prevent displaying validated pixels.
            if originals::keep(dir, key, descriptor, bytes).is_ok() {
                let _ = store.sweep_originals(dir);
            }
        }
    }
    Ok(frame_bytes(frame))
}
async fn original(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    selection: &Selection,
    gate: &std::sync::Mutex<()>,
    manual: bool,
    stop: impl Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if stop() {
        return Err("Identity changed".into());
    }
    let (store, item) = current(dir, identity, selection)?;
    let key = {
        // Media reads do not advance MLS; serialize only first-time sealing-key creation.
        let _guard = gate.lock().map_err(|_| "Session state unavailable")?;
        crate::sealing_key(root, dir, &store)?
    };
    if let Some(bytes) = item
        .sticker
        .download()
        .and_then(|descriptor| originals::read(dir, &key, descriptor))
    {
        return retain(dir, identity, selection, &item, &key, &bytes);
    }
    if let Ok(Some(pack)) = store.saved_sticker_pack(&key, &item.sticker.pack_id) {
        if let Some(index) = pack
            .stickers()
            .iter()
            .position(|entry| entry.sha256 == item.sticker.sha256)
        {
            let bytes = pack.sticker_bytes(index).ok_or("corrupted")?;
            return retain(dir, identity, selection, &item, &key, bytes);
        }
    }
    let descriptor = item.sticker.download().ok_or("notFound")?;
    if !manual {
        let mode = store.auto_download().map_err(|e| e.to_string())?;
        let contact = store
            .conversation(&selection.peer_id)
            .map_err(|e| e.to_string())?
            == Conversation::Open;
        if !attachment::auto_download(mode, chat_photos::metered().await, contact) {
            return Err("onTap".into());
        }
    }
    if stop() {
        return Err("Identity changed".into());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut nodes = profile::reachable(&active).await?;
    if stop() {
        return Err("Identity changed".into());
    }
    let bytes = attachment::fetch(&mut nodes, descriptor)
        .await
        .map_err(chat_photos::code)?;
    if stop() {
        return Err("Identity changed".into());
    }
    retain(dir, identity, selection, &item, &key, &bytes)
}

/// Binary frame: width and height as little-endian u32, followed by bounded RGBA8 pixels.
#[tauri::command]
pub(crate) async fn chat_sticker_frame(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    selection: Selection,
    manual: bool,
) -> Result<tauri::ipc::Response, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = sessions.inner().0.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        tauri::async_runtime::block_on(original(
            &root,
            dir,
            identity,
            &selection,
            &sessions,
            manual,
            || lease.transitioning().unwrap_or(true),
        ))
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[cfg(test)]
#[path = "chat/tests.rs"]
mod tests;
