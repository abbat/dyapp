//! Profile links in chats: Share sends one as an ordinary text, and a card opens the profile it
//! names; see the shared profile section of docs/design/v0.0.1-client-screens.md.

use std::path::Path;
use std::sync::{Arc, Mutex};

use dyapp_client::session::{Session, Submission};
use dyapp_identity::network::{self, Settings};
use dyapp_identity::profile_link::profile_link;
use dyapp_identity::Identity;

use crate::profile;
use crate::search::{Found, ProfileCache, SearchState};

/// The verified profile behind a shared profile link, cached so it can be liked or messaged.
/// Errors are `invalidPeer`, `blocked`, `notConnected`, `unpublished` or a node message.
async fn fetch(
    cache: &Mutex<ProfileCache>,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
) -> Result<Found, String> {
    profile_link(peer_id).ok_or("invalidPeer")?;
    ensure_unblocked(dir, identity, peer_id)?;
    let epoch = cache.lock().map_err(|e| e.to_string())?.bind(dir)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    if !cache.lock().map_err(|e| e.to_string())?.current(epoch)? {
        return Err("notConnected".into());
    }
    let mut node = profile::connect(&active).await?;
    let fetched = node.get_profile(peer_id).await.map_err(|e| e.to_string())?;
    if fetched.profile.deleted || fetched.profile.version == 0 {
        return Err("unpublished".into());
    }
    // The user may have blocked this peer while the network request was pending.
    ensure_unblocked(dir, identity, peer_id)?;
    finish_fetch(cache, epoch, fetched)
}

fn finish_fetch(
    cache: &Mutex<ProfileCache>,
    epoch: u64,
    fetched: dyapp_profile::VerifiedProfile,
) -> Result<Found, String> {
    let mut cache = cache.lock().map_err(|e| e.to_string())?;
    cache
        .with_current(epoch, |context, _| {
            let found = Found::from(&fetched);
            // Reopening an equal version is valid, even though it adds nothing to the cache.
            let _ = context.accept(fetched);
            found
        })?
        .ok_or_else(|| "notConnected".into())
}

fn ensure_unblocked(dir: &Path, identity: &Identity, peer_id: &str) -> Result<(), String> {
    if super::open(dir, identity)?
        .blocked(peer_id)
        .map_err(|e| e.to_string())?
    {
        return Err("blocked".into());
    }
    Ok(())
}

/// Sends `peer_id`'s profile link as an ordinary text into the open chat with `target_peer`.
async fn send(
    cache: &Mutex<ProfileCache>,
    root: &Path,
    dir: &Path,
    identity: &Identity,
    peer_id: &str,
    target_peer: &str,
) -> Result<[u8; 16], String> {
    let store = super::open(dir, identity)?;
    let seal_key = crate::sealing_key(root, dir, &store)?;
    let scores = dyapp_client::contact_verification::scores(dir).map_err(|e| e.to_string())?;
    let mut session = Session::open_with_scores(store, &seal_key, scores).map_err(share_error)?;
    session
        .prepare_profile_share(target_peer, peer_id)
        .map_err(share_error)?;
    let own = share_owner(&session, identity)?;
    let epoch = cache.lock().map_err(|e| e.to_string())?.bind(dir)?;
    let active = network::load(dir, Settings::Active)
        .map_err(|e| e.to_string())?
        .ok_or("notConnected")?;
    let mut node = profile::connect(&active).await?;
    let fetched = node
        .get_profile(target_peer)
        .await
        .map_err(|e| e.to_string())?;
    let peer = super::accept_peer(cache, epoch, target_peer, fetched)?;
    session
        .submit_profile_share_to(
            std::slice::from_mut(&mut node),
            &[own, peer],
            peer_id,
            &seal_key,
        )
        .await
        .and_then(Submission::into_accepted)
        .map_err(share_error)
}

fn share_owner(
    session: &Session,
    identity: &Identity,
) -> Result<dyapp_profile::VerifiedProfile, String> {
    let own = profile::stored(session.store())?;
    if own.version == 0 {
        return Err("unpublished".into());
    }
    let own = dyapp_profile::verify(&own.sign(identity)).map_err(|e| e.to_string())?;
    if !own
        .profile
        .devices
        .contains(&session.certificate().map_err(|e| e.to_string())?)
    {
        return Err("unpublished".into());
    }
    Ok(own)
}

fn share_error(error: dyapp_client::session::Error) -> String {
    match error {
        dyapp_client::session::Error::InvalidProfileShare
        | dyapp_client::session::Error::MissingSnapshot => "invalidTarget".into(),
        dyapp_client::session::Error::Store(dyapp_client::store::Error::BlockedPeer) => {
            "blocked".into()
        }
        error => super::send_photo::code(error),
    }
}

/// The profile a shared link names; see `fetch`.
#[tauri::command]
pub(crate) async fn shared_profile(
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_id: String,
) -> Result<Found, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let cache = Arc::clone(&searching.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        tauri::async_runtime::block_on(fetch(&cache, &dir, &identity, &peer_id))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shares a profile into an open chat; see `send`.
#[tauri::command]
pub(crate) async fn share_profile(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::IdentityState>,
    searching: tauri::State<'_, SearchState>,
    sessions: tauri::State<'_, super::Sessions>,
    peer_id: String,
    target_peer_id: String,
) -> Result<[u8; 16], String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active().clone();
    let root = crate::config_dir(&app)?;
    let cache = Arc::clone(&searching.0);
    let sessions = Arc::clone(&sessions.0);
    tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let _session = sessions.lock().map_err(|e| e.to_string())?;
        tauri::async_runtime::block_on(send(
            &cache,
            &root,
            &dir,
            &identity,
            &peer_id,
            &target_peer_id,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_network_result_is_not_returned_but_reopening_is_allowed() {
        let peer = Identity::try_generate().unwrap();
        let fetched = dyapp_profile::verify(
            &dyapp_profile::Profile {
                version: 1,
                ..dyapp_profile::Profile::default()
            }
            .sign(&peer),
        )
        .unwrap();
        let cache = Mutex::new(ProfileCache::default());
        let epoch = cache.lock().unwrap().epoch();
        assert_eq!(
            finish_fetch(&cache, epoch, fetched.clone())
                .unwrap()
                .peer_id,
            peer.peer_id()
        );
        assert!(finish_fetch(&cache, epoch, fetched.clone()).is_ok());
        cache.lock().unwrap().clear().unwrap();
        assert_eq!(
            finish_fetch(&cache, epoch, fetched).err(),
            Some("notConnected".into())
        );
        assert_eq!(cache.lock().unwrap().snapshot().unwrap().1.all(), []);
    }

    #[tokio::test]
    async fn blocked_profile_is_refused_before_network_or_cache_access() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../build")
            .join(format!("desktop-profile-share-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        let identity = Identity::try_generate().unwrap();
        let peer = Identity::try_generate().unwrap().peer_id();
        let store = super::super::open(&dir, &identity).unwrap();
        let cache = Mutex::default();

        store.set_muted(&peer, true).unwrap();
        assert!(ensure_unblocked(&dir, &identity, &peer).is_ok());
        store.set_blocked(&peer, true).unwrap();
        // There is no active network: notConnected would mean the privacy gate ran too late.
        assert_eq!(
            fetch(&cache, &dir, &identity, &peer).await.err(),
            Some("blocked".into())
        );
        assert_eq!(cache.lock().unwrap().snapshot().unwrap().1.all(), []);
        store.set_blocked(&peer, false).unwrap();
        assert_eq!(
            fetch(&cache, &dir, &identity, &peer).await.err(),
            Some("notConnected".into())
        );
    }
}
