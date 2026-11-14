//! A screen names a current message; media capabilities and peer WebP stay in native code.
use super::owner_matches;
use crate::{chat_photos, chats, profile, IdentityState};
use dyapp_client::{
    attachment,
    sticker_webp::decode::Frame,
    store::{
        messages::{ChatSticker, MessageKey},
        Store,
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

pub(crate) fn selected(store: &Store, selection: &Selection) -> Result<ChatSticker, String> {
    dyapp_client::sticker_media::select(store, &selection.peer_id, &selection.key())
        .map(|snapshot| snapshot.message().clone())
        .map_err(|error| error.to_string())
}
pub(crate) fn frame_bytes(frame: Frame) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + frame.pixels.len());
    bytes.extend(frame.width.to_le_bytes());
    bytes.extend(frame.height.to_le_bytes());
    bytes.extend(frame.pixels);
    bytes
}

async fn fetch(
    dir: &Path,
    descriptor: &attachment::Attachment,
    stop: impl Fn() -> bool,
) -> Result<Vec<u8>, String> {
    if stop() {
        return Err("Identity changed".into());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|error| error.to_string())?
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
    Ok(bytes)
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
    let store = chats::open(dir, identity)?;
    let snapshot =
        dyapp_client::sticker_media::select(&store, &selection.peer_id, &selection.key())
            .map_err(|error| error.to_string())?;
    let key = {
        // Media reads do not advance MLS; serialize only first-time sealing-key creation.
        let _guard = gate.lock().map_err(|_| "Session state unavailable")?;
        crate::sealing_key(root, dir, &store)?
    };
    if let Some(frame) = snapshot
        .local(&store, dir, &key)
        .map_err(|error| error.to_string())?
    {
        return Ok(frame_bytes(frame));
    }
    let descriptor = snapshot.download().ok_or("notFound")?;
    if !manual
        && !snapshot
            .auto_download(&store, chat_photos::metered().await)
            .map_err(|error| error.to_string())?
    {
        return Err("onTap".into());
    }
    let bytes = fetch(dir, descriptor, &stop).await?;
    let current = chats::open(dir, identity)?;
    snapshot
        .finish(&current, dir, &key, &bytes)
        .map(frame_bytes)
        .map_err(|error| error.to_string())
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
