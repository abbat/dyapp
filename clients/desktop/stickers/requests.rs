//! Request status contains local intent metadata, never a peer attachment capability.
use super::{
    chat::{self, Selection},
    local_store, owner_matches,
};
use crate::{chats, IdentityState};
use dyapp_client::{
    sticker_pack::Pack,
    store::{
        sticker_packs::exchange::download::{
            PackDownloadFailure, PackDownloadJob, PackDownloadState,
        },
        Conversation, Store,
    },
};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    state: &'static str,
    generation: Option<[u8; 16]>,
    failure: Option<&'static str>,
    retry_at_ms: Option<u64>,
}
impl Status {
    fn plain(state: &'static str, generation: Option<[u8; 16]>) -> Self {
        Self {
            state,
            generation,
            failure: None,
            retry_at_ms: None,
        }
    }
    fn failed(reason: &'static str, generation: Option<[u8; 16]>) -> Self {
        Self {
            failure: Some(reason),
            ..Self::plain("failed", generation)
        }
    }
    fn download(state: PackDownloadState, token: [u8; 16]) -> Self {
        let failure = state.failure.map(|reason| match reason {
            PackDownloadFailure::Fetch => "fetch",
            PackDownloadFailure::Invalid => "invalid",
            PackDownloadFailure::Quota => "quota",
            PackDownloadFailure::Storage => "storage",
        });
        Self {
            state: if failure.is_some() {
                "failed"
            } else if state.has_answer {
                "downloading"
            } else {
                "waiting"
            },
            generation: Some(token),
            failure,
            retry_at_ms: state.has_answer.then_some(state.retry_at_ms),
        }
    }
}
fn contains(pack: &Pack, hash: &[u8; 32]) -> bool {
    pack.stickers().iter().any(|entry| &entry.sha256 == hash)
}
fn status(store: &mut Store, key: &[u8; 32], selection: &Selection) -> Result<Status, String> {
    let item = chat::selected(store, selection)?;
    if item.key.author != selection.peer_id {
        return Err("unknownMessage".into());
    }
    let pending = store
        .pending_sticker_pack(&selection.peer_id, &item.sticker.pack_id)
        .map_err(|e| e.to_string())?;
    let generation = pending.as_ref().map(|pending| pending.token);
    match store.saved_sticker_pack(key, &item.sticker.pack_id) {
        Ok(Some(pack)) => {
            if !contains(&pack, &item.sticker.sha256) {
                return Ok(Status::failed("stickerMissing", generation));
            }
            if let Some(pending) = pending.as_ref() {
                if contains(&pack, &pending.sticker_hash)
                    && store
                        .conversation(&selection.peer_id)
                        .map_err(|e| e.to_string())?
                        == Conversation::Open
                {
                    let job = PackDownloadJob {
                        peer: selection.peer_id.clone(),
                        pack_id: *pack.id(),
                        sticker_hash: pending.sticker_hash,
                        token: pending.token,
                    };
                    store
                        .save_requested_sticker_pack(key, &selection.group_id, &job, &pack)
                        .map_err(|e| e.to_string())?;
                }
            }
            return Ok(Status::plain("saved", None));
        }
        Ok(None) => {}
        Err(_) => return Ok(Status::failed("storage", generation)),
    }
    let Some(pending) = pending else {
        return Ok(Status::plain("new", None));
    };
    let state = store
        .sticker_pack_download_state(&selection.peer_id, &item.sticker.pack_id)
        .map_err(|e| e.to_string())?
        .ok_or("requestChanged")?;
    Ok(Status::download(state, pending.token))
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
