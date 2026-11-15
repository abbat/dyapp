//! Other people's profile photos: thumbnails and full images from the network's nodes, held in
//! memory only, so nothing on disk says whose profile the user looked at. A blob that is missing or
//! refused is not remembered, so the next view asks again.

pub(crate) use dyapp_client::peer_photo_cache::Cache as PhotoCache;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::attachment;
use dyapp_client::store::profile_photos::{fits, MAX_PHOTO_BYTES, MAX_THUMBNAIL_BYTES};
use dyapp_identity::network::{self, Settings};

use crate::chat_photos::{check_image, metered};
use crate::profile;
use crate::search::{ProfileCache, SearchState};

/// A node returns what a peer uploaded, so the bounds hold again before the page decodes it.
fn acceptable(bytes: &[u8], full: bool) -> bool {
    let limit = if full {
        MAX_PHOTO_BYTES
    } else {
        MAX_THUMBNAIL_BYTES
    };
    fits(bytes, limit) && check_image(bytes).is_ok()
}

/// Fetches what the cache lacks over one set of connections, so a page of cards costs one connect.
/// Errors are `notConnected` or a storage message; a single blob's failure is not one.
async fn fetch(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    hashes: &[Vec<u8>],
    full: bool,
) -> Result<(), String> {
    let (epoch, wanted, generation) = {
        let mut cache = cache.lock().map_err(|e| e.to_string())?;
        let epoch = cache.bind(dir)?;
        let (wanted, generation) = cache
            .with_current(epoch, |context, photos| {
                (photos.missing(context, hashes), context.generation())
            })?
            .ok_or("notConnected")?;
        (epoch, wanted, generation)
    };
    if wanted.is_empty() {
        return Ok(());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut nodes = profile::reachable(&active).await?;
    if nodes.is_empty() {
        return Err("notConnected".into());
    }
    for hash in &wanted {
        for node in &mut nodes {
            // The node checks nothing about the blob but `get_media` checks its hash, so another
            // node cannot do better after a refusal here.
            let Ok(bytes) = node.get_media(hash).await else {
                continue;
            };
            if acceptable(&bytes, full) {
                let mut cache = cache.lock().map_err(|e| e.to_string())?;
                if cache.with_current(epoch, |context, photos| {
                    photos.put(context, &generation, hash.to_vec(), bytes, full)
                })? != Some(true)
                {
                    return Err("notConnected".into());
                }
            }
            break;
        }
    }
    Ok(())
}

/// Loads `hashes` into the memory cache; `full` selects the full-image bounds and limit.
#[tauri::command]
pub(crate) async fn fetch_peer_photos(
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    hashes: Vec<Vec<u8>>,
    full: bool,
) -> Result<(), String> {
    let lease = state.lease()?;
    let dir = lease.active().1.clone();
    let cache = Arc::clone(&searching.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(fetch(&cache, &dir, &hashes, full))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A cached photo's bytes; `notFound` when it was never fetched, was refused or left the cache.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(crate) fn peer_photo(
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    hash: Vec<u8>,
) -> Result<tauri::ipc::Response, String> {
    let lease = state.lease()?;
    let mut cache = searching.0.lock().map_err(|e| e.to_string())?;
    let epoch = cache.bind(&lease.active().1)?;
    let bytes = cache
        .with_current(epoch, |context, photos| photos.get(context, &hash))?
        .flatten()
        .ok_or("notFound")?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Whether full images load without a tap now, under the chat originals' setting. Opening a
/// profile asks for its photos, so it counts as a contact's attachment.
#[tauri::command]
pub(crate) async fn peer_photo_auto(
    state: tauri::State<'_, crate::IdentityState>,
) -> Result<bool, String> {
    let mode = {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        profile::open(dir, identity)?
            .auto_download()
            .map_err(|e| e.to_string())?
    };
    Ok(attachment::auto_download(mode, metered().await, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        RgbImage::new(3, 2).write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn a_blob_outside_the_photo_bounds_is_refused() {
        let jpeg = encoded(ImageFormat::Jpeg);
        assert!(acceptable(&jpeg, false) && acceptable(&jpeg, true));
        assert!(!acceptable(&encoded(ImageFormat::Png), true));
        assert!(!acceptable(&[0xFF, 0xD8, 0xFF, 0xE0], true));
        let mut padded = jpeg;
        padded.resize(MAX_THUMBNAIL_BYTES + 1, 0);
        assert!(!acceptable(&padded, false));
        assert!(acceptable(&padded, true));
        padded.resize(MAX_PHOTO_BYTES + 1, 0);
        assert!(!acceptable(&padded, true));
    }

    #[test]
    fn shared_activation_fences_photo_callbacks_and_discards_old_bytes() {
        let dir =
            std::env::temp_dir().join(format!("dyapp-photo-admission-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cache = ProfileCache::default();
        let epoch = cache.bind(&dir).unwrap();
        let authority = Arc::clone(&cache.context);
        cache
            .with_current(epoch, |context, photos| {
                assert!(authority.try_lock().is_err());
                assert!(photos.put(context, &context.generation(), vec![1], vec![2], false));
                assert!(photos.put(context, &context.generation(), vec![3], vec![4], true));
            })
            .unwrap();
        authority.lock().unwrap().clear();
        assert_eq!(
            cache
                .with_current(epoch, |_, _| panic!("stale photo callback ran"))
                .unwrap(),
            None
        );
        let current = cache.bind(&dir).unwrap();
        assert_eq!(
            cache
                .with_current(current, |context, photos| (
                    photos.get(context, &[1]),
                    photos.get(context, &[3])
                ))
                .unwrap(),
            Some((None, None))
        );
    }

    #[tokio::test]
    async fn only_a_missing_well_formed_hash_needs_the_network_and_context_clears_the_cache() {
        let dir = std::env::temp_dir().join(format!("dyapp-peer-photos-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = Mutex::<ProfileCache>::default();
        let epoch = cache.lock().unwrap().bind(&dir).unwrap();
        let held = vec![7; 32];
        assert_eq!(
            cache
                .lock()
                .unwrap()
                .with_current(epoch, |context, photos| {
                    photos.put(context, &context.generation(), held.clone(), vec![1], false)
                })
                .unwrap(),
            Some(true)
        );
        let known = [held.clone(), vec![1, 2, 3]];
        assert_eq!(fetch(&cache, &dir, &known, false).await, Ok(()));
        let missing = [vec![8; 32]];
        assert_eq!(
            fetch(&cache, &dir, &missing, false).await,
            Err("notConnected".into())
        );
        cache.lock().unwrap().clear().unwrap();
        let mut cache = cache.lock().unwrap();
        let epoch = cache.epoch();
        assert_eq!(
            cache
                .with_current(epoch, |context, photos| photos.get(context, &held))
                .unwrap(),
            Some(None)
        );
    }
}
