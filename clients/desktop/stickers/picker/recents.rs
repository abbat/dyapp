//! Transport-only projection of common core recent choices and cancellation.
use super::{local_store, owner_matches, StickerView};
use crate::{chats, IdentityState};
use dyapp_client::sticker_picker::recents;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    pack_id: [u8; 32],
    pack_name: String,
    #[serde(flatten)]
    sticker: StickerView,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageView {
    entries: Vec<Entry>,
    next: Option<usize>,
}
impl From<recents::Entry> for Entry {
    fn from(value: recents::Entry) -> Self {
        Self {
            pack_id: value.pack_id,
            pack_name: value.pack_name,
            sticker: value.sticker.into(),
        }
    }
}
impl From<recents::Page> for PageView {
    fn from(value: recents::Page) -> Self {
        Self {
            entries: value.entries.into_iter().map(Into::into).collect(),
            next: value.next,
        }
    }
}

/// At most eight recent choices; unavailable packs/entries are filtered without deleting files.
#[tauri::command]
pub(crate) async fn recent_sticker_page(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    after: usize,
) -> Result<PageView, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = chats::Sessions(sessions.inner().0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let (store, key) = local_store(&root, dir, identity, &sessions)?;
        recents::page(&store, &key, after, || {
            lease.transitioning().unwrap_or(true)
        })
        .map(Into::into)
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "recents/tests.rs"]
mod tests;
