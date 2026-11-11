//! The open identity's local privacy settings, blocked, muted and passed peers, kept in its client
//! store and never published. Turning sending off never stops receiving.

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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent privacy toggles map directly to persisted settings and IPC"
)]
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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
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
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn blocked(dir: &Path, identity: &Identity) -> Result<Vec<String>, String> {
    open(dir, identity)?
        .blocked_peers()
        .map_err(|e| e.to_string())
}

/// Peers passed on this device, by Peer ID.
#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub fn passed(dir: &Path, identity: &Identity) -> Result<Vec<String>, String> {
    open(dir, identity)?
        .passed_peers()
        .map_err(|e| e.to_string())
}

#[expect(unreachable_pub, reason = "Retained module API visibility")]
pub mod commands {
    use crate::chats::{self, own, Likes, Sessions};
    use crate::identity_runtime::IdentityState;
    use dyapp_client::chat::PeerChange;

    /// The open identity's privacy settings; see `privacy::load`.
    #[tauri::command]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Tauri extracts command state and request values by value"
    )]
    pub(crate) fn privacy_settings(
        state: tauri::State<'_, IdentityState>,
    ) -> Result<super::Privacy, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::load(dir, identity)
    }

    /// Stores the open identity's privacy settings; see `privacy::save`.
    #[tauri::command]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Tauri extracts command state and request values by value"
    )]
    pub(crate) fn set_privacy(
        state: tauri::State<'_, IdentityState>,
        privacy: super::Privacy,
    ) -> Result<(), String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::save(dir, identity, privacy)
    }

    /// Peers the open identity blocked on this device.
    #[tauri::command]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Tauri extracts command state and request values by value"
    )]
    pub(crate) fn blocked_peers(
        state: tauri::State<'_, IdentityState>,
    ) -> Result<Vec<String>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::blocked(dir, identity)
    }

    /// Shows a peer in later searches again.
    #[tauri::command]
    pub(crate) async fn unblock_peer(
        app: tauri::AppHandle,
        state: tauri::State<'_, IdentityState>,
        sessions: tauri::State<'_, Sessions>,
        peer_id: String,
    ) -> Result<(), String> {
        own::command(&app, &state, &sessions, peer_id, PeerChange::Blocked(false)).await
    }

    /// Mutes a peer or ends that: only notices change, never the mail or the unread counts.
    #[tauri::command]
    pub(crate) async fn set_muted(
        app: tauri::AppHandle,
        state: tauri::State<'_, IdentityState>,
        sessions: tauri::State<'_, Sessions>,
        peer_id: String,
        muted: bool,
    ) -> Result<(), String> {
        own::command(&app, &state, &sessions, peer_id, PeerChange::Muted(muted)).await
    }

    /// Peers the open identity passed on this device.
    #[tauri::command]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Tauri extracts command state and request values by value"
    )]
    pub(crate) fn passed_peers(
        state: tauri::State<'_, IdentityState>,
    ) -> Result<Vec<String>, String> {
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        super::passed(dir, identity)
    }

    /// Passes on a peer or undoes that; no signal goes to the peer.
    #[tauri::command]
    pub(crate) async fn pass_peer(
        app: tauri::AppHandle,
        state: tauri::State<'_, IdentityState>,
        sessions: tauri::State<'_, Sessions>,
        peer_id: String,
        passed: bool,
    ) -> Result<Likes, String> {
        let change = PeerChange::Passed(passed);
        own::command(&app, &state, &sessions, peer_id.clone(), change).await?;
        let lease = state.lease()?;
        let (identity, dir) = lease.active();
        chats::likes(dir, identity, &peer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        // Parallel tests can read the same clock value, notably on Windows.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-privacy-{nanos}-{n}"));
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
}
