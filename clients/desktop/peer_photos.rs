//! Other people's profile photos: thumbnails and full images from the network's nodes, held in
//! memory only, so nothing on disk says whose profile the user looked at. A blob that is missing or
//! refused is not remembered, so the next view asks again.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::attachment;
use dyapp_client::store::profile_photos::{fits, MAX_PHOTO_BYTES, MAX_THUMBNAIL_BYTES};
use dyapp_identity::network::{self, Settings};

use crate::chat_photos::{check_image, metered};
use crate::profile;
use crate::search::{ProfileCache, SearchState};

/// About 6 MiB of thumbnails.
const THUMBNAILS: usize = 256;
/// About 4 MiB of full images.
const FULL_PHOTOS: usize = 8;

type Entry = (Vec<u8>, Vec<u8>);

fn touch(entries: &mut VecDeque<Entry>, hash: &[u8]) -> Option<Vec<u8>> {
    let at = entries.iter().position(|(held, _)| held == hash)?;
    let entry = entries.remove(at)?;
    let bytes = entry.1.clone();
    entries.push_back(entry);
    Some(bytes)
}

/// Blobs by hash, least recently used first. The owner clears it with the profile cache.
// ponytail: linear scans, fine for 264 entries; a hash map index if the limits grow.
#[derive(Default)]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub struct PhotoCache {
    thumbnails: VecDeque<Entry>,
    full: VecDeque<Entry>,
}

impl PhotoCache {
    fn get(&mut self, hash: &[u8]) -> Option<Vec<u8>> {
        touch(&mut self.thumbnails, hash).or_else(|| touch(&mut self.full, hash))
    }

    fn put(&mut self, hash: Vec<u8>, bytes: Vec<u8>, full: bool) {
        let (entries, limit) = if full {
            (&mut self.full, FULL_PHOTOS)
        } else {
            (&mut self.thumbnails, THUMBNAILS)
        };
        entries.retain(|(held, _)| *held != hash);
        if entries.len() == limit {
            entries.pop_front();
        }
        entries.push_back((hash, bytes));
    }
}

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
    let (epoch, wanted) = {
        let mut cache = cache.lock().map_err(|e| e.to_string())?;
        let wanted: Vec<[u8; 32]> = hashes
            .iter()
            .filter(|hash| cache.photos.get(hash).is_none())
            .filter_map(|hash| hash.as_slice().try_into().ok())
            .collect();
        (cache.epoch(), wanted)
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
                if cache.current(epoch) {
                    cache.photos.put(hash.to_vec(), bytes, full);
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
pub(crate) fn peer_photo(
    searching: tauri::State<'_, SearchState>,
    hash: Vec<u8>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = searching
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .photos
        .get(&hash)
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

    fn hash(index: usize) -> Vec<u8> {
        (index as u32).to_be_bytes().to_vec()
    }

    #[test]
    fn the_cache_stays_within_its_limits_and_drops_the_least_recently_used() {
        for (full, limit) in [(false, THUMBNAILS), (true, FULL_PHOTOS)] {
            let mut cache = PhotoCache::default();
            for index in 0..limit {
                cache.put(hash(index), hash(index), full);
            }
            assert_eq!(cache.get(&hash(0)), Some(hash(0)));
            cache.put(hash(limit), hash(limit), full);
            cache.put(hash(limit + 1), hash(limit + 1), full);
            assert_eq!(cache.thumbnails.len() + cache.full.len(), limit);
            assert_eq!(cache.get(&hash(0)), Some(hash(0)));
            assert_eq!(cache.get(&hash(1)), None);
            assert_eq!(cache.get(&hash(2)), None);
        }
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

    #[tokio::test]
    async fn only_a_missing_well_formed_hash_needs_the_network_and_context_clears_the_cache() {
        let dir = std::env::temp_dir().join(format!("dyapp-peer-photos-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cache = Mutex::<ProfileCache>::default();
        let held = vec![7; 32];
        cache
            .lock()
            .unwrap()
            .photos
            .put(held.clone(), vec![1], false);
        let known = [held.clone(), vec![1, 2, 3]];
        assert_eq!(fetch(&cache, &dir, &known, false).await, Ok(()));
        let missing = [vec![8; 32]];
        assert_eq!(
            fetch(&cache, &dir, &missing, false).await,
            Err("notConnected".into())
        );
        cache.lock().unwrap().clear();
        assert_eq!(cache.lock().unwrap().photos.get(&held), None);
    }
}
