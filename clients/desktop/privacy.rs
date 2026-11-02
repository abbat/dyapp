//! The open identity's local privacy settings, blocked and passed peers, kept in its client store
//! and never published. Turning sending off never stops receiving.

use std::path::Path;

use dyapp_client::store::{NoticeContent, Store};
use dyapp_identity::Identity;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(remote = "NoticeContent", rename_all = "camelCase")]
enum NoticeContentDef {
    Fixed,
    Name,
    Text,
}

/// Privacy settings as the page shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Privacy {
    pub send_views: bool,
    pub send_read_receipts: bool,
    pub send_typing: bool,
    /// Presence goes to matches, otherwise to nobody.
    pub presence_matches: bool,
    pub notifications: bool,
    #[serde(with = "NoticeContentDef")]
    pub notice_content: NoticeContent,
}

fn open(dir: &Path, identity: &Identity) -> Result<Store, String> {
    Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())
}

/// The stored settings; unset ones read as their defaults.
pub fn load(dir: &Path, identity: &Identity) -> Result<Privacy, String> {
    let store = open(dir, identity)?;
    let read = || -> Result<Privacy, dyapp_client::store::Error> {
        Ok(Privacy {
            send_views: store.send_profile_views()?,
            send_read_receipts: store.send_read_receipts()?,
            send_typing: store.send_typing()?,
            presence_matches: store.send_presence()?,
            notifications: store.notifications()?,
            notice_content: store.notice_content()?,
        })
    };
    read().map_err(|e| e.to_string())
}

/// Stores every setting of `privacy`.
pub fn save(dir: &Path, identity: &Identity, privacy: Privacy) -> Result<(), String> {
    let store = open(dir, identity)?;
    let write = || -> Result<(), dyapp_client::store::Error> {
        store.set_send_profile_views(privacy.send_views)?;
        store.set_send_read_receipts(privacy.send_read_receipts)?;
        store.set_send_typing(privacy.send_typing)?;
        store.set_send_presence(privacy.presence_matches)?;
        store.set_notifications(privacy.notifications)?;
        store.set_notice_content(privacy.notice_content)
    };
    write().map_err(|e| e.to_string())
}

/// Peers blocked on this device, by Peer ID.
pub fn blocked(dir: &Path, identity: &Identity) -> Result<Vec<String>, String> {
    open(dir, identity)?
        .blocked_peers()
        .map_err(|e| e.to_string())
}

/// Shows `peer_id` in later searches again.
pub fn unblock(dir: &Path, identity: &Identity, peer_id: &str) -> Result<(), String> {
    open(dir, identity)?
        .set_blocked(peer_id, false)
        .map_err(|e| e.to_string())
}

/// Peers passed on this device, by Peer ID.
pub fn passed(dir: &Path, identity: &Identity) -> Result<Vec<String>, String> {
    open(dir, identity)?
        .passed_peers()
        .map_err(|e| e.to_string())
}

pub mod commands {
    use crate::chats::{self, Likes};
    use crate::identity_runtime::IdentityState;

    /// Peers the open identity blocked on this device.
    #[tauri::command]
    pub(crate) fn blocked_peers(
        state: tauri::State<'_, IdentityState>,
    ) -> Result<Vec<String>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::blocked(dir, identity)
    }

    /// Unblocks a peer on this device; see `privacy::unblock`.
    #[tauri::command]
    pub(crate) fn unblock_peer(
        state: tauri::State<'_, IdentityState>,
        peer_id: String,
    ) -> Result<(), String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::unblock(dir, identity, &peer_id)
    }

    /// Peers the open identity passed on this device.
    #[tauri::command]
    pub(crate) fn passed_peers(
        state: tauri::State<'_, IdentityState>,
    ) -> Result<Vec<String>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::passed(dir, identity)
    }

    /// Passes on a peer or undoes that; see `chats::pass`.
    #[tauri::command]
    pub(crate) fn pass_peer(
        state: tauri::State<'_, IdentityState>,
        peer_id: String,
        passed: bool,
    ) -> Result<Likes, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        chats::pass(dir, identity, &peer_id, passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-privacy-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_then_saved_settings_stay_with_their_identity() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let (dir, other) = (temp_dir(), temp_dir());
        let defaults = Privacy {
            send_views: false,
            send_read_receipts: false,
            send_typing: true,
            presence_matches: true,
            notifications: true,
            notice_content: NoticeContent::Fixed,
        };
        assert_eq!(load(&dir, &alice), Ok(defaults));
        let changed = Privacy {
            send_views: true,
            send_read_receipts: true,
            send_typing: false,
            presence_matches: false,
            notifications: false,
            notice_content: NoticeContent::Text,
        };
        save(&dir, &alice, changed).unwrap();
        assert_eq!(load(&dir, &alice), Ok(changed));
        assert_eq!(load(&other, &bob), Ok(defaults));
    }

    #[test]
    fn unblock_returns_a_blocked_peer() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        let peer = Identity::generate().peer_id().to_string();
        open(&dir, &alice)
            .unwrap()
            .set_blocked(&peer, true)
            .unwrap();
        assert_eq!(blocked(&dir, &alice), Ok(vec![peer.clone()]));
        unblock(&dir, &alice, &peer).unwrap();
        assert_eq!(blocked(&dir, &alice), Ok(Vec::new()));
    }
}
