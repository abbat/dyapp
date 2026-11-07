//! Resolve local recent choices against saved packs without holding an unbounded pack cache.
use super::{local_store, owner_matches, thumbnail, StickerView, PAGE_SIZE};
use crate::{chats, IdentityState};
use dyapp_client::{
    sticker_pack::Pack,
    store::{
        sticker_packs::{recents::RecentSticker, Error as PackError},
        Store,
    },
};
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
fn available_pack(store: &Store, key: &[u8; 32], id: &[u8; 32]) -> Result<Option<Pack>, String> {
    match store.saved_sticker_pack(key, id) {
        Ok(pack) => Ok(pack),
        Err(PackError::Invalid) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}
fn page(
    recent: &[RecentSticker],
    after: usize,
    mut load: impl FnMut(&[u8; 32]) -> Result<Option<Pack>, String>,
    stop: impl Fn() -> bool,
) -> Result<PageView, String> {
    if after > recent.len() {
        return Err("invalidIndex".into());
    }
    if stop() {
        return Err("Identity changed".into());
    }
    let mut entries = Vec::new();
    let mut next = None;
    let mut cached_id = None;
    let mut cached: Option<Pack> = None;
    for (offset, record) in recent.iter().enumerate().skip(after) {
        if stop() {
            return Err("Identity changed".into());
        }
        if cached_id != Some(record.pack_id) {
            drop(cached.take());
            cached = load(&record.pack_id)?;
            cached_id = Some(record.pack_id);
        }
        let Some(pack) = cached.as_ref() else {
            continue;
        };
        if pack.id() != &record.pack_id {
            return Err("corrupted".into());
        }
        if stop() {
            return Err("Identity changed".into());
        }
        let Some(index) = pack
            .stickers()
            .iter()
            .position(|entry| entry.sha256 == record.sha256 && entry.emoji == record.emoji)
        else {
            continue;
        };
        if entries.len() == PAGE_SIZE {
            next = Some(offset);
            break;
        }
        entries.push(Entry {
            pack_id: record.pack_id,
            pack_name: pack.name().into(),
            sticker: thumbnail(pack, index)?,
        });
    }
    if stop() {
        return Err("Identity changed".into());
    }
    Ok(PageView { entries, next })
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
        let recent = store.recent_stickers().map_err(|e| e.to_string())?;
        page(
            &recent,
            after,
            |id| available_pack(&store, &key, id),
            || lease.transitioning().unwrap_or(true),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "recents/tests.rs"]
mod tests;
