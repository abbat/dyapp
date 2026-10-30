//! The open identity's local privacy settings and blocked peers, kept in its client store and
//! never published. Turning sending off never stops receiving.

use std::path::Path;

use dyapp_client::store::Store;
use dyapp_identity::Identity;

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
        store.set_notifications(privacy.notifications)
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
        };
        assert_eq!(load(&dir, &alice), Ok(defaults));
        let changed = Privacy {
            send_views: true,
            send_read_receipts: true,
            send_typing: false,
            presence_matches: false,
            notifications: false,
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
