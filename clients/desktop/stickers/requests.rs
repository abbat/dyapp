//! Request status contains local intent metadata, never a peer attachment capability.
use super::{chat::Selection, local_store, owner_matches};
use crate::{chats, IdentityState};
use dyapp_client::{sticker_requests as pack_requests, store::Store};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    state: &'static str,
    generation: Option<[u8; 16]>,
    failure: Option<&'static str>,
    retry_at_ms: Option<u64>,
}
impl From<pack_requests::Status> for Status {
    fn from(value: pack_requests::Status) -> Self {
        Self {
            state: match value.state {
                pack_requests::State::New => "new",
                pack_requests::State::Saved => "saved",
                pack_requests::State::Waiting => "waiting",
                pack_requests::State::Downloading => "downloading",
                pack_requests::State::Failed => "failed",
            },
            generation: value.generation,
            failure: value.failure.map(|failure| match failure {
                pack_requests::Failure::Fetch => "fetch",
                pack_requests::Failure::Invalid => "invalid",
                pack_requests::Failure::Quota => "quota",
                pack_requests::Failure::Storage => "storage",
                pack_requests::Failure::StickerMissing => "stickerMissing",
            }),
            retry_at_ms: value.retry_at_ms,
        }
    }
}
fn status(store: &mut Store, key: &[u8; 32], selection: &Selection) -> Result<Status, String> {
    pack_requests::status(store, key, &selection.peer_id, &selection.key())
        .map(Into::into)
        .map_err(|error| error.to_string())
}

/// Saved status authenticates the pack and checks this clicked sticker, then reconciles valid intent.
#[tauri::command]
pub(crate) async fn chat_sticker_pack_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    selection: Selection,
) -> Result<Status, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = chats::Sessions(sessions.inner().0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let (mut store, key) = local_store(&root, dir, identity, &sessions)?;
        if lease.transitioning().unwrap_or(true) {
            return Err("Identity changed".into());
        }
        status(&mut store, &key, &selection)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[path = "requests/actions.rs"]
pub(crate) mod actions;
#[cfg(test)]
#[path = "requests/tests.rs"]
mod tests;
