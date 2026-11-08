//! Blocks, mutes, contact names and passes, which the owner's devices share, and a device leaving
//! the owner's list.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::chat::PeerChange;
use dyapp_client::profiles::Profiles;
use dyapp_client::session::{Error, Session};
use dyapp_client::store::Store;
use dyapp_client::Node;
use dyapp_identity::Identity;
use tauri::Manager;

use super::{open, Sessions};
use crate::search::ProfileCache;
use crate::IdentityState;

/// Makes `change` about `peer_id` here and, once the owner's devices share a group, stages it for
/// them: the next catch-up sends it, so this needs no network. Hold [`Sessions`] around the call.
/// Invalid notes or their target return `invalidNote`; other invalid changes return `invalidName`.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn change_peer(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
    change: PeerChange,
) -> Result<(), String> {
    if !change.valid(peer_id) {
        return Err(if matches!(change, PeerChange::Note(_)) {
            "invalidNote"
        } else {
            "invalidName"
        }
        .into());
    }
    if matches!(change, PeerChange::Note(_)) && peer_id == identity.peer_id() {
        return Err("invalidNote".into());
    }
    let mut store = open(dir, identity)?;
    let owner = identity.peer_id();
    if let PeerChange::Favorite(favorite) = &change {
        if peer_id == owner {
            return Err("invalidName".into());
        }
        if *favorite && store.blocked(peer_id).map_err(|error| error.to_string())? {
            return Err("blocked".into());
        }
    }
    let groups = store.known_groups().map_err(|e| e.to_string())?;
    if !groups.iter().any(|(_, peer)| *peer == owner) {
        let at = store
            .tick(dyapp_client::now() * 1000)
            .map_err(|e| e.to_string())?;
        return store
            .apply_peer_change(peer_id, &change, at, &[])
            .map_err(|e| e.to_string());
    }
    let device = crate::device_key(root, dir, identity)?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session =
        Session::open_or_create(store, identity, &device, &seal_key).map_err(|e| e.to_string())?;
    session
        .change_peer(&mut [], peer_id, change, &seal_key)
        .await
        .map_err(|e| e.to_string())
}

/// Refreshes one contact's profile, the next in turn, since nothing else shows that a contact
/// dropped a device that stays silent. One request per catch-up: `now` picks the contact, so a lost
/// device of a contact goes within as many minutes as there are chats.
// ponytail: stateless round robin, a stored cursor if a skipped contact ever matters.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub async fn refresh_contact(
    cache: &Mutex<ProfileCache>,
    epoch: u64,
    store: &Store,
    node: &mut Node,
    cached: &mut Profiles,
    now: u64,
) {
    let owner = store.owner_peer_id();
    let mut peers: Vec<_> = store.known_groups().unwrap_or_default();
    peers.retain(|(_, peer)| peer != owner);
    let turn = usize::try_from(now / 60).unwrap_or_default();
    let Some((_, peer)) = peers.get(turn.checked_rem(peers.len()).unwrap_or_default()) else {
        return;
    };
    if let Ok(verified) = node.get_profile(peer).await {
        if let Ok(mut shared) = cache.lock() {
            shared.accept_if_current(epoch, verified.clone());
        }
        let _ = cached.accept(verified);
    }
}

// A catch-up pass resumes exact staged writes after a failed send or restart. Session owns
// outbox backoff; this path never drafts another text.
pub(super) async fn retry_outbox(
    session: &mut Session,
    node: &mut Node,
    seal_key: &[u8; 32],
) -> Result<(), String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let now_ms = i64::try_from(now_ms).map_err(|e| e.to_string())?;
    session
        .retry_due(std::slice::from_mut(node), now_ms, seal_key)
        .await
        .map_err(|e| e.to_string())
}

/// The failure of a catch-up pass that found this device revoked.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub const REVOKED: &str = "revoked";

/// The pass's failure for `error` from reading the mailbox; see `Session::revoked_or`.
pub(super) async fn pass_error(session: &Session, node: &mut Node, error: Error) -> String {
    match session.revoked_or(node, error).await {
        Error::Revoked => REVOKED.to_owned(),
        error => error.to_string(),
    }
}

/// Tells the page to wipe the slot at `dir` after a pass that ended in [`REVOKED`]. The page checks
/// the slot, as another identity may open first. The legacy slot's folder is the config folder,
/// which names no slot, so the page leaves it alone: `slots::remove` refuses it anyway.
pub(super) fn wipe_if_revoked(app: &tauri::AppHandle, dir: &Path, passed: &Result<usize, String>) {
    if !passed.as_ref().is_err_and(|error| error == REVOKED) {
        return;
    }
    let slot = dir.file_name().map(|name| name.to_string_lossy());
    if let (Some(page), Some(slot)) = (app.get_webview_window("main"), slot) {
        let slot = serde_json::Value::from(slot.as_ref());
        let _ = page.eval(format!("window.deviceRevoked?.({slot})"));
    }
}

/// The devices of the active identity; see `profile::devices`.
#[tauri::command]
pub(crate) async fn list_devices(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    live: bool,
) -> Result<Vec<crate::profile::Device>, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    // The client store is not `Sync`, as in `crate::publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let this = crate::device_key(&root, &dir, &identity)?.public_key();
        tauri::async_runtime::block_on(crate::profile::devices(&dir, &identity, &this, live))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Revokes the owner's device with the mailbox key `device_key`; see `profile::revoke_device`.
#[tauri::command]
pub(crate) async fn revoke_device(
    state: tauri::State<'_, IdentityState>,
    device_key: Vec<u8>,
) -> Result<crate::profile::View, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    // The client store is not `Sync`, as in `crate::publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(crate::profile::revoke_device(&dir, &identity, &device_key))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// See `profile::check_link_code`.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(crate) fn check_link_code(code: String) -> Result<(), String> {
    crate::profile::check_link_code(&code)
}

/// Sends the active identity to a new device; see `profile::send_link`.
#[tauri::command]
pub(crate) async fn send_link(
    state: tauri::State<'_, IdentityState>,
    code: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    // The client store is not `Sync`, as in `crate::publish_profile`.
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(crate::profile::send_link(&dir, &identity, &code))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// [`change_peer`] for a command of the active identity, under the session lock.
pub(crate) async fn command(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, IdentityState>,
    sessions: &tauri::State<'_, Sessions>,
    peer_id: String,
    change: PeerChange,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        tauri::async_runtime::block_on(change_peer(&root, &dir, &identity, &peer_id, change))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_dir;
    use super::*;

    #[test]
    fn a_lone_device_applies_a_change_with_its_clock_value() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let peer = Identity::generate().peer_id();
        let change =
            |change| tauri::async_runtime::block_on(change_peer(&dir, &dir, &alice, &peer, change));
        change(PeerChange::Blocked(true)).unwrap();
        assert!(open(&dir, &alice).unwrap().blocked(&peer).unwrap());
        change(PeerChange::Blocked(false)).unwrap();
        let mut store = open(&dir, &alice).unwrap();
        assert!(!store.blocked(&peer).unwrap());
        // A stamped row refuses what a linked device did before this.
        assert!(!store
            .receive_peer_change(&peer, &PeerChange::Blocked(true), 1, 1_000, &[])
            .unwrap());
        drop(store);
        assert_eq!(
            change(PeerChange::Name("a\u{202e}b".to_owned())),
            Err("invalidName".to_owned())
        );
    }
}
