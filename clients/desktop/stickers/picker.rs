//! Bounded picker pages resize validated pixels; peer encodings and capabilities stay native.
use super::{local_store, owner_matches};
use crate::{chats, IdentityState};
use dyapp_client::{sticker_pack::Pack, sticker_picker};
use serde::Serialize;
use sticker_picker::PAGE_SIZE;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StickerView {
    index: usize,
    emoji: String,
    frame: ThumbnailFrame,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageView {
    pack_id: [u8; 32],
    name: String,
    total: usize,
    entries: Vec<StickerView>,
    next: Option<usize>,
}

impl From<sticker_picker::Choice> for StickerView {
    fn from(value: sticker_picker::Choice) -> Self {
        Self {
            index: value.index,
            emoji: value.emoji,
            frame: ThumbnailFrame {
                width: value.frame.width,
                height: value.frame.height,
                pixels: value.frame.pixels,
            },
        }
    }
}
impl From<sticker_picker::Page> for PageView {
    fn from(value: sticker_picker::Page) -> Self {
        Self {
            pack_id: value.pack_id,
            name: value.name,
            total: value.total,
            entries: value.entries.into_iter().map(Into::into).collect(),
            next: value.next,
        }
    }
}

pub(crate) fn thumbnail(pack: &Pack, index: usize) -> Result<StickerView, String> {
    sticker_picker::thumbnail(pack, index)
        .map(Into::into)
        .map_err(|e| e.to_string())
}
fn page(pack: &Pack, after: usize) -> Result<PageView, String> {
    sticker_picker::page(pack, after)
        .map(Into::into)
        .map_err(|e| e.to_string())
}

/// One validated saved pack, with at most eight 96px RGBA thumbnails per response.
#[tauri::command]
pub(crate) async fn saved_sticker_page(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, chats::Sessions>,
    identity_id: String,
    pack_id: [u8; 32],
    after: usize,
) -> Result<PageView, String> {
    let lease = state.lease()?;
    owner_matches(&lease.active().0, &identity_id)?;
    let root = crate::config_dir(&app)?;
    let sessions = chats::Sessions(sessions.inner().0.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let (identity, dir) = lease.active();
        let (store, key) = local_store(&root, dir, identity, &sessions)?;
        let pack = store
            .saved_sticker_pack(&key, &pack_id)
            .map_err(|e| e.to_string())?
            .ok_or("notFound")?;
        if lease.transitioning().unwrap_or(true) {
            return Err("Identity changed".into());
        }
        page(&pack, after)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[path = "picker/recents.rs"]
pub(crate) mod recents;
#[cfg(test)]
#[path = "picker/tests.rs"]
mod tests;
