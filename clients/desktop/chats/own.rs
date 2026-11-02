//! Blocks, mutes, contact names and passes, which the owner's devices share.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::chat::PeerChange;
use dyapp_client::profiles::Profiles;
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_client::Node;
use dyapp_identity::Identity;

use super::{open, Sessions};
use crate::search::ProfileCache;
use crate::IdentityState;

/// Makes `change` about `peer_id` here and, once the owner's devices share a group, stages it for
/// them: the next catch-up sends it, so this needs no network. Hold [`Sessions`] around the call.
/// `invalidName` refuses a name or Peer ID that does not fit.
pub async fn change_peer(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
    change: PeerChange,
) -> Result<(), String> {
    if !change.valid(peer_id) {
        return Err("invalidName".into());
    }
    let mut store = open(dir, identity)?;
    let owner = identity.peer_id();
    let groups = store.known_groups().map_err(|e| e.to_string())?;
    if !groups.iter().any(|(_, peer)| *peer == owner) {
        let at = store
            .tick(dyapp_client::now() * 1000)
            .map_err(|e| e.to_string())?;
        return store
            .apply_peer_change(peer_id, &change, at)
            .map_err(|e| e.to_string());
    }
    let device = crate::device_key(root, dir, identity)?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, &device, &seal_key)
        .map_err(|e| e.to_string())?;
    session
        .change_peer(&mut [], peer_id, change, &seal_key)
        .await
        .map_err(|e| e.to_string())
}

/// Refreshes one contact's profile, the next in turn, since nothing else shows that a contact
/// dropped a device that stays silent. One request per catch-up: `now` picks the contact, so a lost
/// device of a contact goes within as many minutes as there are chats.
// ponytail: stateless round robin, a stored cursor if a skipped contact ever matters.
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
        let change = |change| {
            tauri::async_runtime::block_on(change_peer(&dir, &dir, &alice, &peer, change))
        };
        change(PeerChange::Blocked(true)).unwrap();
        assert!(open(&dir, &alice).unwrap().blocked(&peer).unwrap());
        change(PeerChange::Blocked(false)).unwrap();
        let mut store = open(&dir, &alice).unwrap();
        assert!(!store.blocked(&peer).unwrap());
        // A stamped row refuses what a linked device did before this.
        assert!(!store
            .receive_peer_change(&peer, &PeerChange::Blocked(true), 1, 1_000)
            .unwrap());
        drop(store);
        assert_eq!(
            change(PeerChange::Name("a\u{202e}b".to_owned())),
            Err("invalidName".to_owned())
        );
    }
}
