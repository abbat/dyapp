//! Other people's profile photos, the same rules as `clients/desktop/peer_photos.rs`: thumbnails and
//! full images from the active network's nodes, held in memory by the identity's
//! [`ProfileSearcher`] only, so nothing on disk says whose profile the user looked at. A blob that
//! is missing or refused is not remembered, so the next view asks again.

use crate::preview::{frame_size, network_metered, MAX_SIDE};
use crate::profile_draft::database;
use crate::profile_network::{active_network, on_network_stack};
use crate::profile_search::ProfileSearcher;
use crate::{CoreError, IdentityKey};
use dyapp_client::store::profile_photos::{fits, MAX_PHOTO_BYTES, MAX_THUMBNAIL_BYTES};
use dyapp_client::store::Store;
use dyapp_client::{attachment, Node};
use dyapp_identity::network::Network;
use dyapp_p2p_net::{seed, Protocols};
use std::collections::VecDeque;
use std::sync::Arc;

/// About 6 MiB of thumbnails.
const THUMBNAILS: usize = 256;
/// About 4 MiB of full images.
const FULL_PHOTOS: usize = 8;
/// The desktop's bound on a full image's frame header.
const MAX_PIXELS: u64 = 64_000_000;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

type Entry = (Vec<u8>, Vec<u8>);

fn touch(entries: &mut VecDeque<Entry>, hash: &[u8]) -> Option<Vec<u8>> {
    let at = entries.iter().position(|(held, _)| held == hash)?;
    let entry = entries.remove(at)?;
    let bytes = entry.1.clone();
    entries.push_back(entry);
    Some(bytes)
}

/// Blobs by hash, least recently used first; it lives as long as its searcher.
// ponytail: linear scans, fine for 264 entries; a hash map index if the limits grow.
#[derive(Default)]
pub(crate) struct PhotoCache {
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

/// A node returns what a peer uploaded, so the bounds hold again before the app decodes it: a
/// thumbnail must also pass `check_preview`, a full image stays within 64 megapixels.
pub(crate) fn acceptable(bytes: &[u8], full: bool) -> bool {
    let limit = if full {
        MAX_PHOTO_BYTES
    } else {
        MAX_THUMBNAIL_BYTES
    };
    let Ok((width, height)) = frame_size(bytes) else {
        return false;
    };
    let sized = if full {
        u64::from(width) * u64::from(height) <= MAX_PIXELS
    } else {
        width.max(height) <= MAX_SIDE
    };
    fits(bytes, limit) && width > 0 && height > 0 && sized
}

/// The first answering node of each seed row, as the desktop asks: a blob one node lacks may be
/// on another.
pub(crate) async fn reachable(network: &Network) -> Result<Vec<Node>, CoreError> {
    let protocols = Arc::new(Protocols::new(&network.name).map_err(failed)?);
    let mut nodes = Vec::new();
    for row in &network.seeds {
        for address in seed::parse(row.trim()).into_iter().flatten() {
            if let Ok(node) = Node::connect(address, Arc::clone(&protocols)).await {
                nodes.push(node);
                break;
            }
        }
    }
    Ok(nodes)
}

impl ProfileSearcher {
    async fn fetch_photo(
        &self,
        nodes: &mut [Node],
        hash: &[u8; 32],
        full: bool,
    ) -> Result<(), CoreError> {
        for node in nodes {
            // `get_media` checks the hash, so another node cannot do better after a refusal here.
            let Ok(bytes) = node.get_media(hash).await else {
                continue;
            };
            self.cache_photo(hash, bytes, full)?;
            break;
        }
        Ok(())
    }

    fn cache_photo(&self, hash: &[u8; 32], bytes: Vec<u8>, full: bool) -> Result<(), CoreError> {
        if acceptable(&bytes, full) {
            let mut photos = self.photos.lock().map_err(failed)?;
            photos.put(hash.to_vec(), bytes, full);
        }
        Ok(())
    }
}

#[uniffi::export]
impl ProfileSearcher {
    /// Loads the photos the cache lacks over one set of connections, so a page of cards costs one
    /// connect; `full` selects the full-image bounds and limit. Errors are `notConnected` or a
    /// storage message; a single blob's failure is not one. Blocks; call it off the UI thread.
    ///
    /// # Errors
    ///
    /// Returns an error if cache locking, active settings, network-thread execution,
    /// or connecting fails. Individual blob failures are skipped; photos cached
    /// before a later error remain available.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn fetch_peer_photos(&self, hashes: Vec<Vec<u8>>, full: bool) -> Result<(), CoreError> {
        let wanted: Vec<[u8; 32]> = {
            let mut photos = self.photos.lock().map_err(failed)?;
            hashes
                .iter()
                .filter(|hash| photos.get(hash).is_none())
                .filter_map(|hash| hash.as_slice().try_into().ok())
                .collect()
        };
        if wanted.is_empty() {
            return Ok(());
        }
        let _operation = self.admit(None)?;
        let network = active_network(&self.dir)?;
        on_network_stack(|| {
            self.runtime.block_on(async {
                let mut nodes = reachable(&network).await?;
                if nodes.is_empty() {
                    return Err(failed("notConnected"));
                }
                for hash in &wanted {
                    self.fetch_photo(&mut nodes, hash, full).await?;
                }
                Ok(())
            })
        })
    }

    /// A cached photo's bytes; `notFound` when it was never fetched, was refused or left the cache.
    ///
    /// # Errors
    ///
    /// Returns an error if the cache lock is poisoned or the photo is not cached.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned arguments across the native language boundary"
    )]
    pub fn peer_photo(&self, hash: Vec<u8>) -> Result<Vec<u8>, CoreError> {
        self.photos
            .lock()
            .map_err(failed)?
            .get(&hash)
            .ok_or_else(|| failed("notFound"))
    }
}

/// Whether full peer photos load without a tap now, under the chat originals' setting and the
/// platform's metered report. Opening a profile asks for its photos, so it counts as a contact's
/// attachment.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the identity directory, database, or download setting
/// cannot be opened, read, or validated.
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn peer_photo_auto(dir: String, key: Arc<IdentityKey>) -> Result<bool, CoreError> {
    let store = Store::open(&database(&dir)?.join("client.db"), &key.identity).map_err(failed)?;
    let mode = store.auto_download().map_err(failed)?;
    Ok(attachment::auto_download(mode, network_metered(), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::network::{self, Settings};
    use image::codecs::jpeg::JpegEncoder;
    use image::RgbImage;

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        JpegEncoder::new(&mut bytes)
            .encode_image(&RgbImage::new(width, height))
            .unwrap();
        bytes
    }

    fn hash(index: usize) -> Vec<u8> {
        u32::try_from(index).unwrap().to_be_bytes().to_vec()
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
        let small = jpeg(3, 2);
        assert!(acceptable(&small, false) && acceptable(&small, true));
        let wide = jpeg(MAX_SIDE + 1, 2);
        assert!(!acceptable(&wide, false) && acceptable(&wide, true));
        assert!(!acceptable(&[0xFF, 0xD8, 0xFF, 0xD9], true));
        assert!(!acceptable(b"GIF89a", true));
        // Over the thumbnail size but a whole JPEG: padding goes before the end marker.
        let mut padded = small[..small.len() - 2].to_vec();
        padded.resize(MAX_THUMBNAIL_BYTES + 1, 0);
        padded.extend_from_slice(&[0xFF, 0xD9]);
        assert!(!acceptable(&padded, false) && acceptable(&padded, true));
    }

    #[test]
    fn cached_photos_survive_only_in_memory_without_retaining_an_identity_lease() {
        use crate::slot_registry_tests::{root, Credentials};
        use crate::{MobileIdentityRegistry, MobileSlotLayout};
        let root = root();
        let registry = MobileIdentityRegistry::new(
            root.to_str().unwrap().to_owned(),
            MobileSlotLayout::Android,
            Arc::new(Credentials::default()),
        );
        let created = registry.create().unwrap();
        let worker = ProfileSearcher::for_slot(
            registry.clone(),
            created.slot.id.clone(),
            created.key.peer_id(),
        )
        .unwrap();
        let hash = vec![7; 32];
        worker
            .photos
            .lock()
            .unwrap()
            .put(hash.clone(), vec![1], false);
        registry
            .remove(created.slot.id, Some(created.key.peer_id()))
            .unwrap();
        assert_eq!(worker.peer_photo(hash.clone()).unwrap(), vec![1]);
        assert!(worker.fetch_peer_photos(vec![hash], false).is_ok());
        assert!(!std::path::Path::new(&created.slot.directory).exists());
    }

    #[test]
    fn only_a_missing_well_formed_hash_needs_the_network() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/native-peer-photo-tests")
            .join(IdentityKey::generate().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.to_str().unwrap().to_owned();
        let searcher = ProfileSearcher::new(path.clone()).unwrap();
        let held = vec![7; 32];
        searcher
            .photos
            .lock()
            .unwrap()
            .put(held.clone(), vec![1], false);
        let known = vec![held.clone(), vec![1, 2, 3]];
        assert!(searcher.fetch_peer_photos(known, false).is_ok());
        let missing = || searcher.fetch_peer_photos(vec![vec![8; 32]], false);
        assert_eq!(missing().unwrap_err().to_string(), "notConnected");
        let empty = Network {
            seeds: Vec::new(),
            name: String::new(),
        };
        network::save(&dir, Settings::Active, &empty).unwrap();
        assert_eq!(missing().unwrap_err().to_string(), "notConnected");
        assert_eq!(searcher.peer_photo(held).unwrap(), vec![1]);
        assert_eq!(
            searcher.peer_photo(vec![8; 32]).unwrap_err().to_string(),
            "notFound"
        );
        // The metered flag is process-wide and other tests set it, so only the call is checked.
        assert!(peer_photo_auto(path, IdentityKey::generate()).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
