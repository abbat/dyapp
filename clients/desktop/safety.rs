#![expect(unreachable_pub, reason = "Generated Tauri wrapper reexports")]
//! Contact safety-number command for a verified profile shown by the desktop client.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::IdentityState;
use crate::search::{Found, ProfileCache, SearchState};
use dyapp_client::contact_verification::{self, Ticket};
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;
use dyapp_profile::VerifiedProfile;

#[cfg(target_os = "linux")]
#[expect(unreachable_pub, reason = "Retained module API visibility")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri requires an owned Webview in its permission callback"
)]
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

pub use dyapp_client::qr::QrMatrix;

fn matrix_for_peer_ids(mine: &str, theirs: &str) -> Result<QrMatrix, String> {
    let payload = dyapp_identity::safety::qr_payload_for_peer_ids(mine, theirs)
        .ok_or_else(|| "invalid Peer ID".to_owned())?;
    matrix(&payload)
}

/// The QR matrix of `payload`.
pub(crate) fn matrix(payload: &[u8]) -> Result<QrMatrix, String> {
    dyapp_client::qr::matrix(payload).ok_or_else(|| "could not encode contact QR".to_owned())
}

/// Returns a QR matrix for the active identity and selected contact.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
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
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
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
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
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
    ticket: Ticket,
    store: &Store,
    fetched: VerifiedProfile,
) -> Result<Found, String> {
    let peer = cache
        .lock()
        .map_err(|e| e.to_string())?
        .context
        .lock()
        .map_err(|e| e.to_string())?
        .confirm(store, ticket, fetched)
        .map_err(|e| e.to_string())?;
    Ok(Found::from(&peer))
}

/// Fetches `peer_id`'s signed profile from every reachable seed of the active network and marks
/// the selected ordinary version verified for its current devices. An observed final account
/// tombstone dominates live forks and refuses confirmation. The version is the newest observed:
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
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    let (ticket, active) = {
        // Activation uses this same guard; never pair an old ticket with new settings.
        let mut cache = cache.lock().map_err(|e| e.to_string())?;
        cache.bind(dir)?;
        let context = cache.context.lock().map_err(|e| e.to_string())?;
        let ticket = context.begin(&store, peer_id).map_err(|e| e.to_string())?;
        let active = network::load(dir, Settings::Active)
            .map_err(|e| e.to_string())?
            .ok_or("notConnected")?;
        (ticket, active)
    };
    let scores = dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?;
    let mut scores = scores.lock().await;
    let fetched = contact_verification::fetch(&active, peer_id, &mut scores)
        .await
        .map_err(|e| e.to_string())?;
    mark(cache, ticket, &store, fetched)
}

/// Records the caller's confirmed comparison with `peer_id`; see `confirm`.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
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
    Ok(cache
        .lock()
        .map_err(|e| e.to_string())?
        .context
        .lock()
        .map_err(|e| e.to_string())?
        .state(store, peer_id)
        .map_err(|e| e.to_string())?
        .verified)
}

/// Reads the mark for the profile on screen; see `marked`.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn contact_verified(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    searching.0.lock().map_err(|e| e.to_string())?.bind(dir)?;
    marked(&searching.0, &store, &peer_id)
}

/// Whether the contact's chat shows the new-device notice. Reading the mark first raises it for
/// a cached profile that added a device.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn contact_new_device(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    searching.0.lock().map_err(|e| e.to_string())?.bind(dir)?;
    Ok(searching
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .context
        .lock()
        .map_err(|e| e.to_string())?
        .state(&store, &peer_id)
        .map_err(|e| e.to_string())?
        .new_device)
}

/// Ends the notice after the user acknowledged it in the chat.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn acknowledge_new_device(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    contact_verification::acknowledge_devices(&store, &peer_id).map_err(|e| e.to_string())
}

/// Takes the mark back, so a mistaken confirmation can be undone without the network.
#[expect(unreachable_pub, reason = "Retained Tauri export visibility")]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub fn unverify_contact(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())?;
    contact_verification::unverify(&store, &peer_id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{confirm, mark, marked, matrix_for_peer_ids, scan_frame, QrMatrix, Ticket};
    use crate::search::ProfileCache;
    use dyapp_client::store::Store;
    use dyapp_identity::network::{self, Network, Settings};
    use dyapp_identity::Identity;
    use dyapp_profile::{Profile, VerifiedProfile};
    use qrcodegen::{QrCode, QrCodeEcc};
    use std::sync::Mutex;

    fn grayscale(matrix: &QrMatrix) -> (usize, Vec<u8>) {
        let modules_per_edge = usize::try_from(matrix.size).unwrap();
        let side = (modules_per_edge + 8) * 6;
        let mut pixels = vec![255; side * side];
        for (index, black) in matrix.modules.iter().enumerate() {
            if *black {
                let x = (index % modules_per_edge + 4) * 6;
                let y = (index / modules_per_edge + 4) * 6;
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
        assert_eq!(
            shown.modules.len(),
            usize::try_from(shown.size * shown.size).unwrap()
        );
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

    fn begin(cache: &Mutex<ProfileCache>, store: &Store, peer: &str) -> Ticket {
        cache
            .lock()
            .unwrap()
            .context
            .lock()
            .unwrap()
            .begin(store, peer)
            .unwrap()
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
        let (me, bob) = (
            Identity::try_generate().unwrap(),
            Identity::try_generate().unwrap(),
        );
        let (dir, store) = temp_store("marked", &me);
        let peer = bob.peer_id();
        let cache = Mutex::<ProfileCache>::default();
        let current = signed(&bob, 3);
        store.set_verified(&current).unwrap();
        assert!(!marked(&cache, &store, &peer).unwrap());
        assert!(cache.lock().unwrap().accept_if_current(0, current).unwrap());
        assert!(marked(&cache, &store, &peer).unwrap());
        store.clear_verified(&peer).unwrap();
        assert!(!marked(&cache, &store, &peer).unwrap());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_current_published_profile_gets_the_verified_mark() {
        let (me, bob, carol) = (
            Identity::from_secret(&[1; 32]),
            Identity::from_secret(&[2; 32]),
            Identity::from_secret(&[3; 32]),
        );
        let (dir, store) = temp_store("mark", &me);
        let peer = bob.peer_id();
        let cache = Mutex::<ProfileCache>::default();
        let current = signed(&bob, 3);
        cache
            .lock()
            .unwrap()
            .accept_if_current(0, signed(&bob, 2))
            .unwrap();

        let refused = [
            (signed(&carol, 3), "unknownPeer"),
            (signed(&bob, 1), "staleProfile"),
        ];
        for (fetched, error) in refused {
            let ticket = begin(&cache, &store, &peer);
            assert_eq!(mark(&cache, ticket, &store, fetched), Err(error.into()));
        }
        assert!(!store.verified(&current).unwrap());

        let ticket = begin(&cache, &store, &peer);
        let found = mark(&cache, ticket, &store, current.clone()).unwrap();
        assert_eq!(found.version, 3);
        assert!(store.verified(&current).unwrap());

        let dave = Identity::from_secret(&[4; 32]);
        let ticket = begin(&cache, &store, &dave.peer_id());
        cache.lock().unwrap().clear().unwrap();
        assert_eq!(
            mark(&cache, ticket, &store, signed(&dave, 1)),
            Err("notConnected".into())
        );
        let tombstone = Profile {
            previous: Some(dyapp_profile::ProfileCondition::default()),
            ..Profile::tombstone(2, 0, 0)
        };
        let tombstone = dyapp_profile::verify(&tombstone.sign(&dave)).unwrap();
        let ticket = begin(&cache, &store, &dave.peer_id());
        assert_eq!(
            mark(&cache, ticket, &store, tombstone.clone()),
            Err("unpublished".into())
        );
        assert!(!store.verified(&tombstone).unwrap());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn terminal_authority_hides_the_mark_and_invalid_targets_are_rejected() {
        let (me, bob) = (
            Identity::from_secret(&[1; 32]),
            Identity::from_secret(&[2; 32]),
        );
        let (dir, store) = temp_store("terminal", &me);
        let cache = Mutex::<ProfileCache>::default();
        let peer = bob.peer_id();
        let ticket = begin(&cache, &store, &peer);
        mark(&cache, ticket, &store, signed(&bob, 3)).unwrap();
        assert!(marked(&cache, &store, &peer).unwrap());
        let tombstone = Profile {
            previous: Some(dyapp_profile::ProfileCondition::default()),
            ..Profile::tombstone(2, 0, 0)
        };
        cache
            .lock()
            .unwrap()
            .context
            .lock()
            .unwrap()
            .accept(dyapp_profile::verify(&tombstone.sign(&bob)).unwrap())
            .unwrap();
        assert!(!marked(&cache, &store, &peer).unwrap());
        assert_eq!(marked(&cache, &store, &me.peer_id()), Err("ownPeer".into()));
        assert_eq!(marked(&cache, &store, "bad"), Err("invalid Peer ID".into()));
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn confirmation_needs_a_reachable_seed_and_another_valid_peer() {
        let (me, bob) = (
            Identity::from_secret(&[1; 32]),
            Identity::from_secret(&[2; 32]),
        );
        let (dir, store) = temp_store("offline", &me);
        let cache = Mutex::default();
        let (own, peer) = (me.peer_id(), bob.peer_id());
        let run = |peer_id| confirm(&cache, &dir, &me, peer_id);
        assert_eq!(run(&own).await, Err("ownPeer".into()));
        assert_eq!(run("nope").await, Err("invalid Peer ID".into()));
        assert_eq!(run(&peer).await, Err("notConnected".into()));
        let offline = Network {
            seeds: vec!["127.0.0.1:0".into()],
            name: String::new(),
        };
        network::save(&dir, Settings::Active, &offline).unwrap();
        assert_eq!(run(&peer).await, Err("notConnected".into()));
        assert!(!store.verified(&signed(&bob, 1)).unwrap());
        drop(store);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
