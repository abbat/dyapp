//! Private contact notes stay in the active identity's store and own-device groups.
use crate::chats::{self, own, Sessions};
use crate::IdentityState;
use dyapp_client::chat::PeerChange;
use dyapp_identity::Identity;
use std::path::Path;

fn read(dir: &Path, identity: &Identity, peer: &str) -> Result<String, String> {
    dyapp_client::contact_notes::read(&chats::open(dir, identity)?, peer)
        .map_err(|error| error.to_string())
}

/// Local private text; reading never fetches a profile or contacts a node.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri extracts command state and request values by value"
)]
pub(crate) fn contact_note(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
) -> Result<String, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    read(dir, identity, &peer_id)
}

/// Empty text clears through the same stamped own-device change as an edit.
#[tauri::command]
pub(crate) async fn set_contact_note(
    app: tauri::AppHandle,
    state: tauri::State<'_, IdentityState>,
    sessions: tauri::State<'_, Sessions>,
    peer_id: String,
    note: String,
) -> Result<(), String> {
    own::command(&app, &state, &sessions, peer_id, PeerChange::Note(note)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> std::path::PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/desktop-notes")
            .join(Identity::generate().peer_id());
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn offline_notes_are_private_bounded_and_identity_scoped() {
        let (dir, owner, peer) = (
            directory(),
            Identity::generate(),
            Identity::generate().peer_id(),
        );
        assert_eq!(read(&dir, &owner, &peer).unwrap(), "");
        own::change_peer(
            &dir,
            &dir,
            &owner,
            &peer,
            PeerChange::Note("Case\nLine".into()),
        )
        .await
        .unwrap();
        assert_eq!(read(&dir, &owner, &peer).unwrap(), "Case\nLine");
        for note in [
            "hidden\u{202e}".into(),
            "x".repeat(dyapp_client::chat::MAX_NOTE_CHARS + 1),
        ] {
            assert_eq!(
                own::change_peer(&dir, &dir, &owner, &peer, PeerChange::Note(note)).await,
                Err("invalidNote".into())
            );
        }
        assert_eq!(read(&dir, &owner, &peer).unwrap(), "Case\nLine");
        let store = chats::open(&dir, &owner).unwrap();
        assert!(store.due_outbound(0).unwrap().is_empty());
        store.set_blocked(&peer, true).unwrap();
        own::change_peer(&dir, &dir, &owner, &peer, PeerChange::Note(String::new()))
            .await
            .unwrap();
        assert_eq!(read(&dir, &owner, &peer).unwrap(), "");
        assert!(store.due_outbound(0).unwrap().is_empty());
        assert_eq!(
            read(&directory(), &Identity::generate(), &peer).unwrap(),
            ""
        );
        assert!(read(&dir, &owner, "bad").is_err());
        assert!(read(&dir, &owner, &owner.peer_id()).is_err());
        reject_self_note(&dir, &owner).await;
    }

    async fn reject_self_note(dir: &Path, owner: &Identity) {
        assert_eq!(
            own::change_peer(
                dir,
                dir,
                owner,
                &owner.peer_id(),
                PeerChange::Note("self".into())
            )
            .await,
            Err("invalidNote".into())
        );
    }
}
