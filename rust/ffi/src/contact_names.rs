//! Peer names and main photos from the searcher's profile cache, by the desktop rule: a local name,
//! else the public name with its mark, else none.

use crate::profile_network::{active_network, connect, on_network_stack};
use crate::profile_search::{FoundProfile, ProfileSearcher};
use crate::{CoreError, IdentityKey};
use dyapp_client::store::{ContactName, Store};
use dyapp_profile::VerifiedProfile;
use std::path::Path;
use std::sync::Arc;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// How a screen names one peer.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ContactNameView {
    /// The peer's ID.
    pub peer_id: String,
    /// The local or public name; empty means no name, so the short Peer ID shows.
    pub name: String,
    /// The mark of a public name; absent for a local name and for no name.
    pub verified: Option<bool>,
    /// The hash of the first thumbnail; empty when no photo is known.
    pub photo: Vec<u8>,
}

/// The verified profiles of `peer_ids` from the active network, in that order; a peer no node
/// returns is left out.
pub(crate) fn fetch_profiles(
    runtime: &tokio::runtime::Runtime,
    dir: &Path,
    peer_ids: Vec<String>,
) -> Result<Vec<VerifiedProfile>, CoreError> {
    if peer_ids.is_empty() {
        return Ok(Vec::new());
    }
    let network = active_network(dir)?;
    on_network_stack(|| {
        runtime.block_on(async {
            let mut node = connect(&network).await?;
            let mut found = Vec::new();
            for peer_id in peer_ids {
                if let Ok(verified) = node.get_profile(&peer_id).await {
                    found.push(verified);
                }
            }
            Ok(found)
        })
    })
}

fn view(
    store: &Store,
    peer: Option<&VerifiedProfile>,
    peer_id: String,
) -> Result<ContactNameView, CoreError> {
    let photo = peer.and_then(|peer| peer.profile.photos.first());
    let photo = photo.map_or_else(Vec::new, |photo| photo.thumbnail.clone());
    let name = match peer {
        Some(peer) => store.contact_name(peer).map_err(failed)?,
        None => match store.local_name(&peer_id).map_err(failed)? {
            local if local.is_empty() => ContactName::Unnamed,
            local => ContactName::Local(local),
        },
    };
    let (name, verified) = match name {
        ContactName::Local(name) => (name, None),
        ContactName::Public { name, verified } => (name, Some(verified)),
        ContactName::Unnamed => (String::new(), None),
    };
    Ok(ContactNameView {
        peer_id,
        name,
        verified,
        photo,
    })
}

#[uniffi::export]
impl ProfileSearcher {
    /// The name and main photo of each of `peer_ids`, in that order. A public name and the photo
    /// need the peer's profile in this searcher's memory, from a search or [`Self::profiles`], so
    /// after a restart they return with the next fetch; a local name is always there.
    ///
    /// # Errors
    ///
    /// Returns an error if cache locking, opening the identity database, or reading
    /// local names and verification state fails.
    pub fn contact_names(
        &self,
        key: Arc<IdentityKey>,
        peer_ids: Vec<String>,
    ) -> Result<Vec<ContactNameView>, CoreError> {
        let cache = self.cache.lock().map_err(failed)?;
        let store = Store::open(&self.dir.join("client.db"), &key.identity).map_err(failed)?;
        peer_ids
            .into_iter()
            .map(|peer_id| view(&store, cache.get(&peer_id), peer_id))
            .collect()
    }

    /// The verified profiles of `peer_ids` from the active network, in that order, kept in memory
    /// for [`Self::contact_names`]; a peer no node returns is left out. No peers means no
    /// connection. Errors are `notConnected` or a node message.
    ///
    /// # Errors
    ///
    /// Returns an error if active settings or network-worker execution fails, no
    /// entry node answers, or cache locking fails. Individual unavailable profiles
    /// are omitted from the result.
    pub fn profiles(&self, peer_ids: Vec<String>) -> Result<Vec<FoundProfile>, CoreError> {
        let found = fetch_profiles(&self.runtime, &self.dir, peer_ids)?;
        let mut cache = self.cache.lock().map_err(failed)?;
        Ok(found
            .into_iter()
            .map(|verified| {
                let profile = FoundProfile::from(&verified);
                // An older or equal version is already cached.
                let _ = cache.accept(verified);
                profile
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dyapp_identity::Identity;
    use dyapp_profile::{Photo, Profile};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn a_local_name_wins_and_a_public_name_needs_a_cached_profile_and_carries_its_mark() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dyapp-ffi-names-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let key = IdentityKey::generate();
        let bob = Identity::generate();
        let peer = bob.peer_id();
        let searcher = ProfileSearcher::new(dir.to_str().unwrap().to_owned()).unwrap();
        let store = Store::open(&dir.join("client.db"), &key.identity).unwrap();
        let read = || {
            let views = searcher.contact_names(key.clone(), vec![peer.clone()]);
            views.unwrap().remove(0)
        };
        let named = |name: &str, verified| (name.to_owned(), verified);
        let shown = |view: ContactNameView| (view.name, view.verified);

        assert_eq!(shown(read()), named("", None));
        store.set_local_name(&peer, "Bobby").unwrap();
        assert_eq!(shown(read()), named("Bobby", None));
        store.set_local_name(&peer, "").unwrap();

        let profile = Profile {
            version: 3,
            name: "Bob".to_owned(),
            photos: vec![Photo {
                full: vec![vec![1; 32]],
                thumbnail: vec![2; 32],
            }],
            ..Default::default()
        };
        let current = dyapp_profile::verify(&profile.sign(&bob)).unwrap();
        searcher
            .cache
            .lock()
            .unwrap()
            .accept(current.clone())
            .unwrap();
        assert_eq!(shown(read()), named("Bob", Some(false)));
        assert_eq!(read().photo, vec![2; 32]);
        store.set_verified(&current).unwrap();
        assert_eq!(shown(read()), named("Bob", Some(true)));
        store.set_local_name(&peer, "Bobby").unwrap();
        assert_eq!(shown(read()), named("Bobby", None));

        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
