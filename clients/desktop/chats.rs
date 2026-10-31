//! Likes and the Liked you list of the open identity on its active network, over the client
//! core's `Session`; see docs/design/chats-and-matches.md.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::chat::Event;
use dyapp_client::interactions::Kind;
use dyapp_client::profiles::Profiles;
use dyapp_client::session::Session;
use dyapp_client::store::Store;
use dyapp_identity::network::{self, Settings};
use dyapp_identity::Identity;

use crate::profile;
use crate::search::Found;

/// Held while an MLS session is open: two sessions would advance the same saved state, and the
/// later commit would drop what the other did.
#[derive(Default)]
pub struct Sessions(pub Arc<Mutex<()>>);

/// The like state with one peer as the page shows it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Likes {
    /// This profile likes the peer: the page shows Liked.
    pub ours: bool,
    /// Both like each other.
    pub matched: bool,
}

fn open(dir: &Path, identity: &Identity) -> Result<Store, String> {
    Store::open(&dir.join("client.db"), identity).map_err(|e| e.to_string())
}

fn likes_in(store: &Store, peer_id: &str) -> Result<Likes, String> {
    let likes = store.likes(peer_id).map_err(|e| e.to_string())?;
    Ok(Likes {
        ours: likes.ours,
        matched: likes.matched(),
    })
}

/// The like state with `peer_id`.
pub fn likes(dir: &Path, identity: &Identity, peer_id: &str) -> Result<Likes, String> {
    likes_in(&open(dir, identity)?, peer_id)
}

/// Likes `peer_id` (`liked`) or takes the like back, as a signal in the pair group on the active
/// network; the first like invites the peer's devices. Only a peer a search or Liked you list of
/// this run returned can be liked. A send that fails stays staged with its bytes. Errors are
/// `unknownPeer`, `unpublished` (no published own profile lists this device), `notConnected` or
/// a node or storage message.
pub async fn like(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
    peer_id: &str,
    liked: bool,
) -> Result<Likes, String> {
    let peer = cache
        .lock()
        .map_err(|e| e.to_string())?
        .get(peer_id)
        .cloned()
        .ok_or("unknownPeer")?;
    let store = open(dir, identity)?;
    let own = profile::stored(&store)?;
    if own.version == 0 {
        return Err("unpublished".into());
    }
    let own = dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let seal_key = profile::seal_key(device_key);
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    let kind = if liked { Kind::Like } else { Kind::Unlike };
    session
        .interact_with(
            std::slice::from_mut(&mut node),
            &[own, peer],
            kind,
            &seal_key,
        )
        .await
        .map_err(|e| e.to_string())?;
    likes_in(session.store(), peer_id)
}

/// The Liked you peers (`Store::liked_by`) with their profiles verified from the active network,
/// cached so they can be liked or blocked; a peer whose profile no node returns is left out. With
/// no such peer it does not connect. Errors are `notConnected` or a storage message.
pub async fn liked_you(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
) -> Result<Vec<Found>, String> {
    let peers = open(dir, identity)?.liked_by().map_err(|e| e.to_string())?;
    if peers.is_empty() {
        return Ok(Vec::new());
    }
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let mut found = Vec::new();
    for peer_id in peers {
        let Ok(verified) = node.get_profile(&peer_id).await else {
            continue;
        };
        found.push(Found::from(&verified));
        // An older or equal version is already cached.
        let _ = cache.lock().map_err(|e| e.to_string())?.accept(verified);
    }
    Ok(found)
}

/// Fetches this device's mailbox on the active network and applies it with `Session::receive`,
/// knowing the cached profiles and the published own one. An envelope from an owner not cached,
/// such as the Welcome of a first like, gets that owner's profile from the node and is applied
/// on a second pass; one still unknown waits in the mailbox for the next call. Returns how many
/// events changed a chat. Errors are `notConnected` or a node or storage message.
pub async fn receive(
    cache: &Mutex<Profiles>,
    dir: &Path,
    identity: &Identity,
    device_key: &Identity,
) -> Result<usize, String> {
    let store = open(dir, identity)?;
    let own = profile::stored(&store)?;
    let own = match own.version {
        0 => None,
        _ => Some(dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?),
    };
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let seal_key = profile::seal_key(device_key);
    let mut session = Session::open_or_create(store, identity, device_key, &seal_key)
        .map_err(|e| e.to_string())?;
    let mut changed = 0;
    for _ in 0..2 {
        let mut profiles = cache.lock().map_err(|e| e.to_string())?.all().to_vec();
        profiles.extend(own.clone());
        let events = session
            .receive(
                std::slice::from_mut(&mut node),
                device_key,
                &profiles,
                &seal_key,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut fetched = false;
        for event in events {
            match event {
                Event::UnknownOwner(peer) => {
                    if let Ok(verified) = node.get_profile(&peer).await {
                        fetched |= cache
                            .lock()
                            .map_err(|e| e.to_string())?
                            .accept(verified)
                            .is_ok();
                    }
                }
                Event::Gap(_) | Event::Refused(_) => {}
                _ => changed += 1,
            }
        }
        if !fetched {
            break;
        }
    }
    Ok(changed)
}

/// The page's commands for likes and the mailbox; each one that opens an MLS session holds
/// `Sessions` on its blocking thread.
pub mod commands {
    use std::sync::Arc;

    use crate::search::{Found, SearchState};

    /// The like state with a peer; see `likes`.
    #[tauri::command]
    pub(crate) fn peer_likes(
        state: tauri::State<'_, crate::IdentityState>,
        peer_id: String,
    ) -> Result<super::Likes, String> {
        let (identity, dir) = crate::open_identity(&state)?;
        super::likes(&dir, &identity, &peer_id)
    }

    /// Likes a found peer or takes the like back; see `like`.
    #[tauri::command]
    pub(crate) async fn like_peer(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
        sessions: tauri::State<'_, super::Sessions>,
        peer_id: String,
        liked: bool,
    ) -> Result<super::Likes, String> {
        let (identity, dir) = crate::open_identity(&state)?;
        let root = crate::config_dir(&app)?;
        let cache = Arc::clone(&searching.0);
        let sessions = Arc::clone(&sessions.0);
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || {
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            let device = crate::device_key(&root, &dir)?;
            tauri::async_runtime::block_on(super::like(
                &cache, &dir, &identity, &device, &peer_id, liked,
            ))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// The peers whose like is unanswered; see `liked_you`.
    #[tauri::command]
    pub(crate) async fn liked_you(
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
    ) -> Result<Vec<Found>, String> {
        let (identity, dir) = crate::open_identity(&state)?;
        let cache = Arc::clone(&searching.0);
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || {
            tauri::async_runtime::block_on(super::liked_you(&cache, &dir, &identity))
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Fetches and applies this device's mailbox; see `receive`.
    #[tauri::command]
    pub(crate) async fn receive_mail(
        app: tauri::AppHandle,
        state: tauri::State<'_, crate::IdentityState>,
        searching: tauri::State<'_, SearchState>,
        sessions: tauri::State<'_, super::Sessions>,
    ) -> Result<usize, String> {
        let (identity, dir) = crate::open_identity(&state)?;
        let root = crate::config_dir(&app)?;
        let cache = Arc::clone(&searching.0);
        let sessions = Arc::clone(&sessions.0);
        // The client store is not `Sync`, as in `crate::publish_profile`.
        tauri::async_runtime::spawn_blocking(move || {
            let _session = sessions.lock().map_err(|e| e.to_string())?;
            let device = crate::device_key(&root, &dir)?;
            tauri::async_runtime::block_on(super::receive(&cache, &dir, &identity, &device))
        })
        .await
        .map_err(|e| e.to_string())?
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
        let dir = std::env::temp_dir().join(format!("dyapp-desktop-chats-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn only_a_returned_peer_can_be_liked_and_no_like_needs_no_network() {
        let (dir, alice, cache) = (temp_dir(), Identity::generate(), Mutex::default());
        let stranger = Identity::generate().peer_id().to_string();
        let device = Identity::generate();
        assert_eq!(
            like(&cache, &dir, &alice, &device, &stranger, true).await,
            Err("unknownPeer".into())
        );
        assert_eq!(liked_you(&cache, &dir, &alice).await, Ok(Vec::new()));
        assert_eq!(
            receive(&cache, &dir, &alice, &device).await,
            Err("notConnected".into())
        );
        assert_eq!(
            likes(&dir, &alice, &stranger),
            Ok(Likes {
                ours: false,
                matched: false
            })
        );
    }
}
