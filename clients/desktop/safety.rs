//! Contact safety-number command for a verified profile shown by the desktop client.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::IdentityState;
use crate::search::{Found, ProfileCache, SearchState};
use dyapp_client::scores::Scores;
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_profile::VerifiedProfile;
use qrcodegen::{QrCode, QrCodeEcc};

#[cfg(unix)]
pub fn camera_permission(
    webview: tauri::Webview,
    kind: tauri::webview::PermissionKind,
) -> tauri::webview::PermissionResponse {
    match kind {
        tauri::webview::PermissionKind::Camera if webview.label() == "main" => {
            tauri::webview::PermissionResponse::Allow
        }
        _ => tauri::webview::PermissionResponse::Default,
    }
}

#[derive(serde::Serialize)]
pub struct QrMatrix {
    size: i32,
    modules: Vec<bool>,
}

fn matrix_for_peer_ids(mine: &str, theirs: &str) -> Result<QrMatrix, String> {
    let payload = dyapp_identity::safety::qr_payload_for_peer_ids(mine, theirs)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    matrix(&payload)
}

/// The QR matrix of `payload`.
pub(crate) fn matrix(payload: &[u8]) -> Result<QrMatrix, String> {
    let qr = QrCode::encode_binary(payload, QrCodeEcc::Medium)
        .map_err(|_| "could not encode contact QR".to_owned())?;
    let size = qr.size();
    let qr = &qr;
    let modules = (0..size)
        .flat_map(|y| (0..size).map(move |x| qr.get_module(x, y)))
        .collect();
    Ok(QrMatrix { size, modules })
}

/// Returns a QR matrix for the active identity and selected contact.
#[tauri::command]
pub fn contact_safety_qr(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<QrMatrix, String> {
    let lease = state.lease()?;
    matrix_for_peer_ids(&lease.active().0.peer_id(), &peer_id)
}

fn scan_frame(
    width: usize,
    height: usize,
    pixels: &[u8],
    mine: &str,
    theirs: &str,
) -> Result<&'static str, String> {
    const MAX_SIDE: usize = 768;
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || pixels.len() != width * height
    {
        return Err("invalid QR image dimensions".to_owned());
    }
    let expected = dyapp_identity::safety::qr_payload_for_peer_ids(theirs, mine)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    let mut decoder = quircs::Quirc::default();
    let mut result = None;
    for code in decoder.identify(width, height, pixels) {
        if result.is_some() {
            return Ok("multiple");
        }
        let status = match code.ok().and_then(|code| code.decode().ok()) {
            None => "unreadable",
            Some(data)
                if data.payload.len() != expected.len()
                    || data.payload.first() != expected.first() =>
            {
                "invalid"
            }
            Some(data)
                if dyapp_identity::safety::qr_matches_peer_ids(&data.payload, mine, theirs) =>
            {
                "matched"
            }
            Some(_) => "foreign",
        };
        result = Some(status);
    }
    Ok(result.unwrap_or("notFound"))
}

/// Decodes one bounded grayscale frame against the active identity and selected contact.
/// A match is only a comparison result; it never writes a verified-contact mark.
#[tauri::command]
pub fn scan_contact_qr(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
    width: usize,
    height: usize,
    pixels: Vec<u8>,
) -> Result<String, String> {
    let lease = state.lease()?;
    scan_frame(
        width,
        height,
        &pixels,
        &lease.active().0.peer_id(),
        &peer_id,
    )
    .map(str::to_owned)
}

/// Computes a comparison number from this identity and the found profile's Peer ID.
#[tauri::command]
pub fn contact_safety_number(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<String, String> {
    let lease = state.lease()?;
    dyapp_identity::safety::safety_number_for_peer_ids(&lease.active().0.peer_id(), &peer_id)
        .ok_or_else(|| "invalid Peer ID".to_owned())
}

/// Writes the verified mark for `fetched`, the newest profile observed for `peer_id`. The cache
/// lock covers the write, so network activation cannot pass between the check and the mark.
fn mark(
    cache: &Mutex<ProfileCache>,
    epoch: u64,
    store: &Store,
    peer_id: &str,
    fetched: VerifiedProfile,
) -> Result<Found, String> {
    let peer = crate::chats::accept_peer(cache, epoch, peer_id, fetched)?;
    if peer.profile.deleted || peer.profile.version == 0 {
        return Err("unpublished".into());
    }
    let cache = cache.lock().map_err(|e| e.to_string())?;
    if !cache.current(epoch) {
        return Err("notConnected".into());
    }
    store.set_verified(&peer).map_err(|e| e.to_string())?;
    Ok(Found::from(&peer))
}

/// Fetches `peer_id`'s signed profile from every reachable seed of the active network and marks
/// the newest version verified for its current devices. That version is the newest observed:
/// a node set can hide a later one. Errors are `ownPeer`, `invalid Peer ID`, `notConnected` (no
/// active network, no reachable seed, or the network changed), `unavailable` (no seed returned
/// a profile), `unknownPeer`, `staleProfile` (below the version seen in this context),
/// `unpublished` or a storage message; none of them writes a mark.
async fn confirm(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
) -> Result<Found, String> {
    let own = identity.peer_id();
    if peer_id == own {
        return Err("ownPeer".into());
    }
    if dyapp_identity::safety::safety_number_for_peer_ids(&own, peer_id).is_none() {
        return Err("invalid Peer ID".into());
    }
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    // Capture the context before loading settings: activation saves settings and advances this
    // epoch under the same cache lock.
    let epoch = cache.lock().map_err(|e| e.to_string())?.epoch();
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut nodes = crate::profile::reachable(&active).await?;
    if nodes.is_empty() {
        return Err("notConnected".into());
    }
    let fetched = dyapp_client::profiles::newest(&mut Scores::default(), &mut nodes, peer_id)
        .await
        .map_err(|_| "unavailable")?;
    mark(cache, epoch, &store, peer_id, fetched)
}

/// Records the caller's confirmed comparison with `peer_id`; see `confirm`.
#[tauri::command]
pub async fn confirm_contact(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<Found, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let cache = Arc::clone(&searching.0);
    // The client store is not `Sync`, so the work runs on a thread of its own.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(confirm(&cache, &dir, &identity, &peer_id))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether the owner of the cached profile is marked verified. With no cached profile the page
/// shows no mark until a search or chat fetches one.
fn marked(cache: &Mutex<ProfileCache>, store: &Store, peer_id: &str) -> Result<bool, String> {
    let (_, profiles) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    match profiles.get(peer_id) {
        Some(peer) => store.verified(peer).map_err(|e| e.to_string()),
        None => Ok(false),
    }
}

/// Reads the mark for the profile on screen; see `marked`.
#[tauri::command]
pub fn contact_verified(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    marked(&searching.0, &store, &peer_id)
}

/// Whether the contact's chat shows the new-device notice. Reading the mark first raises it for
/// a cached profile that added a device.
#[tauri::command]
pub fn contact_new_device(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    marked(&searching.0, &store, &peer_id)?;
    store.new_device(&peer_id).map_err(|e| e.to_string())
}

/// Ends the notice after the user acknowledged it in the chat.
#[tauri::command]
pub fn acknowledge_new_device(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    store
        .acknowledge_new_device(&peer_id)
        .map_err(|e| e.to_string())
}

/// Takes the mark back, so a mistaken confirmation can be undone without the network.
#[tauri::command]
pub fn unverify_contact(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    store.clear_verified(&peer_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{confirm, mark, marked, matrix_for_peer_ids, scan_frame, QrMatrix};
    use crate::search::ProfileCache;
    use dyapp_client::store::Store;
    use dyapp_identity::network::{self, Network, Settings};
    use dyapp_identity::Identity;
    use dyapp_profile::{Profile, VerifiedProfile};
    use qrcodegen::{QrCode, QrCodeEcc};
    use std::sync::Mutex;

    fn grayscale(matrix: &QrMatrix) -> (usize, Vec<u8>) {
        let size = matrix.size as usize;
        let side = (size + 8) * 6;
        let mut pixels = vec![255; side * side];
        for (index, black) in matrix.modules.iter().enumerate() {
            if *black {
                let x = (index % size + 4) * 6;
                let y = (index / size + 4) * 6;
                for row in y..y + 6 {
                    pixels[row * side + x..row * side + x + 6].fill(0);
                }
            }
        }
        (side, pixels)
    }

    #[test]
    fn qr_matrix_binds_both_ordered_peer_ids() {
        let alice = "aa".repeat(32);
        let bob = "22".repeat(32);
        let carol = "33".repeat(32);
        let shown = matrix_for_peer_ids(&alice, &bob).unwrap();
        assert_eq!(shown.modules.len(), (shown.size * shown.size) as usize);
        assert!(shown.modules[0]); // top-left finder pattern
        assert_ne!(
            shown.modules,
            matrix_for_peer_ids(&bob, &alice).unwrap().modules
        );
        assert_ne!(
            shown.modules,
            matrix_for_peer_ids(&alice, &carol).unwrap().modules
        );
        assert!(matrix_for_peer_ids(&alice.to_uppercase(), &bob).is_err());
    }

    #[test]
    fn qr_scan_rejects_wrong_pair_bad_version_and_multiple_codes() {
        let alice = "11".repeat(32);
        let bob = "22".repeat(32);
        let carol = "33".repeat(32);
        let matrix = matrix_for_peer_ids(&bob, &alice).unwrap();
        let (side, pixels) = grayscale(&matrix);
        assert_eq!(
            scan_frame(side, side, &pixels, &alice, &bob).unwrap(),
            "matched"
        );
        assert_eq!(
            scan_frame(side, side, &pixels, &bob, &alice).unwrap(),
            "foreign"
        );
        assert_eq!(
            scan_frame(side, side, &pixels, &alice, &carol).unwrap(),
            "foreign"
        );
        assert_eq!(
            scan_frame(side, side, &vec![255; side * side], &alice, &bob).unwrap(),
            "notFound"
        );
        assert!(scan_frame(side, side, &pixels[..side], &alice, &bob).is_err());
        assert!(scan_frame(769, side, &pixels, &alice, &bob).is_err());

        let mut bad = dyapp_identity::safety::qr_payload_for_peer_ids(&bob, &alice).unwrap();
        bad[0] = 2;
        let qr = QrCode::encode_binary(&bad, QrCodeEcc::Medium).unwrap();
        let qr = &qr;
        let bad_matrix = QrMatrix {
            size: qr.size(),
            modules: (0..qr.size())
                .flat_map(|y| (0..qr.size()).map(move |x| qr.get_module(x, y)))
                .collect(),
        };
        let (bad_side, bad_pixels) = grayscale(&bad_matrix);
        assert_eq!(
            scan_frame(bad_side, bad_side, &bad_pixels, &alice, &bob).unwrap(),
            "invalid"
        );

        let wide = side * 2;
        let mut two = vec![255; wide * side];
        for y in 0..side {
            let row = &pixels[y * side..(y + 1) * side];
            two[y * wide..y * wide + side].copy_from_slice(row);
            two[y * wide + side..(y + 1) * wide].copy_from_slice(row);
        }
        assert_eq!(
            scan_frame(wide, side, &two, &alice, &bob).unwrap(),
            "multiple"
        );
    }

    fn signed(owner: &Identity, version: u64) -> VerifiedProfile {
        let profile = Profile {
            version,
            ..Default::default()
        };
        dyapp_profile::verify(&profile.sign(owner)).unwrap()
    }

    fn temp_store(name: &str, identity: &Identity) -> (std::path::PathBuf, Store) {
        let dir = std::env::temp_dir().join(format!("dyapp-safety-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(&dir.join("client.db"), identity).unwrap();
        (dir, store)
    }

    #[test]
    fn the_mark_is_read_for_a_cached_profile_and_can_be_taken_back() {
        let (me, bob) = (Identity::generate(), Identity::generate());
        let (dir, store) = temp_store("marked", &me);
        let peer = bob.peer_id();
        let cache = Mutex::<ProfileCache>::default();
        let current = signed(&bob, 3);
        store.set_verified(&current).unwrap();
        assert!(!marked(&cache, &store, &peer).unwrap());
        assert!(cache.lock().unwrap().accept_if_current(0, current));
        assert!(marked(&cache, &store, &peer).unwrap());
        store.clear_verified(&peer).unwrap();
        assert!(!marked(&cache, &store, &peer).unwrap());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_current_published_profile_gets_the_verified_mark() {
        let (me, bob, carol) = (
            Identity::generate(),
            Identity::generate(),
            Identity::generate(),
        );
        let (dir, store) = temp_store("mark", &me);
        let peer = bob.peer_id();
        let cache = Mutex::<ProfileCache>::default();
        let current = signed(&bob, 3);
        cache.lock().unwrap().accept_if_current(0, signed(&bob, 2));

        let refused = [
            (signed(&carol, 3), "unknownPeer"),
            (signed(&bob, 1), "staleProfile"),
        ];
        for (fetched, error) in refused {
            assert_eq!(mark(&cache, 0, &store, &peer, fetched), Err(error.into()));
        }
        assert!(!store.verified(&current).unwrap());

        let found = mark(&cache, 0, &store, &peer, current.clone()).unwrap();
        assert_eq!(found.version, 3);
        assert!(store.verified(&current).unwrap());

        cache.lock().unwrap().clear();
        let dave = Identity::generate();
        assert_eq!(
            mark(&cache, 0, &store, &dave.peer_id(), signed(&dave, 1)),
            Err("notConnected".into())
        );
        let tombstone = dyapp_profile::verify(&Profile::tombstone(2, 0, 0).sign(&dave)).unwrap();
        let epoch = cache.lock().unwrap().epoch();
        assert_eq!(
            mark(&cache, epoch, &store, &dave.peer_id(), tombstone.clone()),
            Err("unpublished".into())
        );
        assert!(!store.verified(&tombstone).unwrap());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn confirmation_needs_a_reachable_seed_and_another_valid_peer() {
        let (me, bob) = (Identity::generate(), Identity::generate());
        let (dir, store) = temp_store("offline", &me);
        let cache = Mutex::default();
        let (own, peer) = (me.peer_id(), bob.peer_id());
        let run = |peer_id| confirm(&cache, &dir, &me, peer_id);
        assert_eq!(run(&own).await, Err("ownPeer".into()));
        assert_eq!(run("nope").await, Err("invalid Peer ID".into()));
        assert_eq!(run(&peer).await, Err("notConnected".into()));
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let offline = Network {
            seeds: vec![closed.to_string()],
            name: String::new(),
        };
        network::save(&dir, Settings::Active, &offline).unwrap();
        assert_eq!(run(&peer).await, Err("notConnected".into()));
        assert!(!store.verified(&signed(&bob, 1)).unwrap());
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
