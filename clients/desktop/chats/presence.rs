//! Coarse presence with matches; see docs/design/presence.md.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;

pub(super) fn now_ms() -> Result<u64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    u64::try_from(now.as_millis()).map_err(|e| e.to_string())
}

fn due(store: &Store) -> Result<bool, String> {
    let now = now_ms()?;
    for row in store.chat_rows().map_err(|e| e.to_string())? {
        let draft = store
            .draft_presence(&row.peer_id, now)
            .map_err(|e| e.to_string())?;
        if draft.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Send the presence due to matches and return whether any was. With none due, which is always
/// the case within the hour or at the Nobody setting, this never touches the network.
async fn announce(
    root: &Path,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
) -> Result<bool, String> {
    let store = super::open(dir, identity)?;
    if !due(&store)? {
        return Ok(false);
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    session
        .announce_presence(std::slice::from_mut(&mut node), &seal_key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// The page calls this when the app comes to the foreground. No runtime pass or timer may: presence
/// follows real use.
#[tauri::command]
pub(crate) async fn announce_presence(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    sessions: tauri::State<'_, super::Sessions>,
) -> Result<bool, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        let device = crate::device_key(&root, &dir, &identity)?;
        tauri::async_runtime::block_on(announce(&root, &dir, &identity, &device))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_dir;
    use super::*;
    use dyapp_client::interactions::{Interaction, Kind};

    #[test]
    fn nothing_due_never_reaches_the_network() {
        let (dir, alice) = (temp_dir(), Identity::generate());
        drop(super::super::open(&dir, &alice).unwrap());
        // No network file exists, so reaching the connect step would return notConnected.
        let device = Identity::generate();
        let sent = announce(&dir, &dir, &alice, &device);
        assert_eq!(tauri::async_runtime::block_on(sent), Ok(false));
    }

    #[test]
    fn a_chat_shows_a_match_active_after_its_presence() {
        let (dir, alice, bob) = (temp_dir(), Identity::generate(), Identity::generate());
        let (alice_id, bob_id) = (alice.peer_id(), bob.peer_id());
        let mut store = super::super::open(&dir, &alice).unwrap();
        for (sender, target, device, event_id, wall_ms) in [
            (&alice_id, &bob_id, [1; 32], [1; 16], 1_000),
            (&bob_id, &alice_id, [2; 32], [2; 16], 2_000),
        ] {
            let interaction = Interaction {
                kind: Kind::Like,
                event_id,
                hlc: wall_ms << 16,
            };
            store
                .apply_interaction(sender, target, &device, interaction)
                .unwrap();
        }
        let shown = |id: &str| {
            super::super::chat_history(&dir, &alice, id)
                .unwrap()
                .active_recently
        };
        assert!(!shown(&bob_id));
        let now = now_ms().unwrap();
        assert!(store.receive_presence(&bob_id, now << 16, now).unwrap());
        drop(store);
        assert!(shown(&bob_id));
    }
}
