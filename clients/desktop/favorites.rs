//! Local favorite reads; changes use the owner's sealed-device path, never a peer signal.
use crate::chats::{self, own, Sessions};
use crate::IdentityState;
use dyapp_client::chat::PeerChange;
use dyapp_identity::Identity;
use std::path::Path;

fn list(dir: &Path, identity: &Identity) -> Result<Vec<String>, String> {
    chats::open(dir, identity)?
        .favorite_peers()
        .map_err(|error| error.to_string())
}

fn saved(dir: &Path, identity: &Identity, peer: &str) -> Result<bool, String> {
    if !PeerChange::Favorite(false).valid(peer) || peer == identity.peer_id() {
        return Err("invalidName".into());
    }
    chats::open(dir, identity)?
        .favorite(peer)
        .map_err(|error| error.to_string())
}

/// Saved Peer IDs of the active identity, without blocked peers or network requests.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(crate) fn favorite_peers(
    state: tauri::State<'_, IdentityState>,
) -> Result<Vec<String>, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    list(dir, identity)
}

/// Whether a peer is saved, including a blocked one so removal remains possible.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(crate) fn favorite_state(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    saved(dir, identity, &peer_id)
}

/// Saves or removes a favorite through the existing synchronized own-device command.
#[tauri::command]
pub(crate) async fn set_favorite(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    peer_id: String,
    favorite: bool,
) -> Result<(), String> {
    own::command(
        &app,
        &state,
        &sessions,
        peer_id,
        PeerChange::Favorite(favorite),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/desktop-favorites")
            .join(Identity::generate().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn local_reads_and_synchronized_changes_respect_identity_and_blocks() {
        let (dir, owner, peer) = (
            directory(),
            Identity::generate(),
            Identity::generate().peer_id(),
        );
        assert_eq!(list(&dir, &owner).unwrap(), Vec::<String>::new());
        assert!(!saved(&dir, &owner, &peer).unwrap());
        own::change_peer(&dir, &dir, &owner, &peer, PeerChange::Favorite(true))
            .await
            .unwrap();
        assert_eq!(list(&dir, &owner).unwrap(), std::slice::from_ref(&peer));
        let store = chats::open(&dir, &owner).unwrap();
        assert!(store.due_outbound(0).unwrap().is_empty());
        store.set_blocked(&peer, true).unwrap();
        assert_eq!(list(&dir, &owner).unwrap(), Vec::<String>::new());
        assert!(saved(&dir, &owner, &peer).unwrap());
        assert_eq!(
            own::change_peer(&dir, &dir, &owner, &peer, PeerChange::Favorite(true)).await,
            Err("blocked".into())
        );
        // No network or fetched profile exists: removal must still work.
        own::change_peer(&dir, &dir, &owner, &peer, PeerChange::Favorite(false))
            .await
            .unwrap();
        assert!(!saved(&dir, &owner, &peer).unwrap());
        assert_eq!(
            list(&directory(), &Identity::generate()).unwrap(),
            Vec::<String>::new()
        );
        assert!(saved(&dir, &owner, "bad").is_err());
        assert!(saved(&dir, &owner, &owner.peer_id()).is_err());
        assert_eq!(
            own::change_peer(
                &dir,
                &dir,
                &owner,
                &owner.peer_id(),
                PeerChange::Favorite(true)
            )
            .await,
            Err("invalidName".into())
        );
    }
}
