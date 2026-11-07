//! Bounded picker pages resize validated pixels; peer encodings and capabilities stay native.
use super::{local_store, owner_matches};
use crate::{chats, IdentityState};
use dyapp_client::sticker_pack::Pack;
use serde::Serialize;

const PAGE_SIZE: usize = 8;
const THUMBNAIL_SIDE: u32 = 96;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Pixels {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StickerView {
    index: usize,
    emoji: String,
    frame: Pixels,
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

pub(crate) fn thumbnail(pack: &Pack, index: usize) -> Result<StickerView, String> {
    let entry = pack.stickers().get(index).ok_or("invalidIndex")?;
    let frame = dyapp_client::sticker_webp::decode::first_frame(
        pack.sticker_bytes(index).ok_or("invalidIndex")?,
    )
    .map_err(|_| "corrupted")?;
    let rgba =
        image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels).ok_or("corrupted")?;
    let small = image::imageops::thumbnail(&rgba, THUMBNAIL_SIDE, THUMBNAIL_SIDE);
    Ok(StickerView {
        index,
        emoji: entry.emoji.clone(),
        frame: Pixels {
            width: small.width(),
            height: small.height(),
            pixels: small.into_raw(),
        },
    })
}
fn page(pack: &Pack, after: usize) -> Result<PageView, String> {
    let total = pack.stickers().len();
    if after > total {
        return Err("invalidIndex".into());
    }
    let end = (after + PAGE_SIZE).min(total);
    Ok(PageView {
        pack_id: *pack.id(),
        name: pack.name().into(),
        total,
        entries: (after..end)
            .map(|index| thumbnail(pack, index))
            .collect::<Result<_, _>>()?,
        next: (end < total).then_some(end),
    })
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
