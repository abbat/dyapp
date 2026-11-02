//! The names the page shows for peers and the local name this profile gives a contact.

use std::sync::Mutex;

use dyapp_client::store::{ContactName, Error, Store};

use crate::search::{ProfileCache, SearchState};
use crate::IdentityState;

/// A peer's name for the page. `verified` is absent for a local name and for no name, which carry
/// no mark; an empty `name` means no name.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerNameView {
    pub peer_id: String,
    pub name: String,
    pub verified: Option<bool>,
    /// The hash of the first thumbnail; empty when no photo is known.
    pub photo: Vec<u8>,
}

/// The name and photo of each peer. A public name and the photo need the peer's profile in the
/// in-memory cache, so after a restart they return with the next search or chat fetch; a local name
/// is always there.
fn names(
    cache: &Mutex<ProfileCache>,
    store: &Store,
    peer_ids: Vec<String>,
) -> Result<Vec<PeerNameView>, String> {
    let (_, profiles) = cache.lock().map_err(|e| e.to_string())?.snapshot();
    let mut views = Vec::with_capacity(peer_ids.len());
    for peer_id in peer_ids {
        let peer = profiles.get(&peer_id);
        let photo = peer.and_then(|peer| peer.profile.photos.first());
        let photo = photo.map_or_else(Vec::new, |photo| photo.thumbnail.clone());
        let name = match peer {
            Some(peer) => store.contact_name(peer).map_err(|e| e.to_string())?,
            None => match store.local_name(&peer_id).map_err(|e| e.to_string())? {
                local if local.is_empty() => ContactName::Unnamed,
                local => ContactName::Local(local),
            },
        };
        let (name, verified) = match name {
            ContactName::Local(name) => (name, None),
            ContactName::Public { name, verified } => (name, Some(verified)),
            ContactName::Unnamed => (String::new(), None),
        };
        views.push(PeerNameView {
            peer_id,
            name,
            verified,
            photo,
        });
    }
    Ok(views)
}

/// Names for the peers on screen, for the active identity.
#[tauri::command]
pub fn contact_names(
    state: tauri::State<'_, IdentityState>,
    searching: tauri::State<'_, SearchState>,
    peer_ids: Vec<String>,
) -> Result<Vec<PeerNameView>, String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = super::open(dir, identity)?;
    names(&searching.0, &store, peer_ids)
}

/// Gives a peer with a visible chat a local name; an empty name removes it.
#[tauri::command]
pub fn set_contact_name(
    state: tauri::State<'_, IdentityState>,
    peer_id: String,
    name: String,
) -> Result<(), String> {
    let lease = state.lease()?;
    let (identity, dir) = lease.active();
    let store = super::open_visible_chat(dir, identity, &peer_id)?;
    store
        .set_local_name(&peer_id, name.trim())
        .map_err(|error| match error {
            Error::InvalidName => "invalidName".to_owned(),
            error => error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::{Photo, Profile};

    #[test]
    fn a_local_name_wins_and_a_public_name_needs_a_profile_and_carries_its_mark() {
        let (me, bob) = (Identity::generate(), Identity::generate());
        let dir = super::super::tests::temp_dir();
        let store = Store::open(&dir.join("client.db"), &me).unwrap();
        let cache = Mutex::<ProfileCache>::default();
        let peer = bob.peer_id();
        let read = || names(&cache, &store, vec![peer.clone()]).unwrap().remove(0);
        let view = |name: &str, verified| PeerNameView {
            peer_id: peer.clone(),
            name: name.to_owned(),
            verified,
            photo: Vec::new(),
        };

        assert_eq!(read(), view("", None));
        store.set_local_name(&peer, "Bobby").unwrap();
        assert_eq!(read(), view("Bobby", None));
        store.set_local_name(&peer, "").unwrap();

        let profile = Profile {
            version: 3,
            name: "Bob".to_owned(),
            ..Default::default()
        };
        let current = dyapp_profile::verify(&profile.sign(&bob)).unwrap();
        assert!(cache.lock().unwrap().accept_if_current(0, current.clone()));
        assert_eq!(read(), view("Bob", Some(false)));
        store.set_verified(&current).unwrap();
        assert_eq!(read(), view("Bob", Some(true)));
        store.set_local_name(&peer, "Bobby").unwrap();
        assert_eq!(read(), view("Bobby", None));

        let photos = vec![Photo {
            full: vec![vec![1; 32]],
            thumbnail: vec![2; 32],
        }];
        let newer = Profile {
            version: 4,
            photos,
            ..profile
        };
        let newer = dyapp_profile::verify(&newer.sign(&bob)).unwrap();
        assert!(cache.lock().unwrap().accept_if_current(0, newer));
        assert_eq!(read().photo, vec![2; 32]);

        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
