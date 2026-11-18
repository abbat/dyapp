//! Shared cache-first admission and validated retention; adapters only schedule transport.

use super::{stored, validate, LookupError};
use crate::attachment::Attachment;
use crate::store::{originals, Store};
use std::path::Path;

/// A current stored photo bound to its identity and full message selector.
pub struct Snapshot {
    owner: String,
    peer: String,
    group: Vec<u8>,
    author: String,
    id: Vec<u8>,
    photo: Attachment,
}

/// Admit a stored original before any cache access or transport.
///
/// # Errors
/// Returns the same refusals as [`stored`].
pub fn select(
    store: &Store,
    peer: &str,
    group: &[u8],
    author: &str,
    id: &[u8],
) -> Result<Snapshot, LookupError> {
    let photo = stored(store, peer, group, author, id)?;
    Ok(Snapshot {
        owner: store.owner_peer_id().into(),
        peer: peer.into(),
        group: group.into(),
        author: author.into(),
        id: id.into(),
        photo,
    })
}

impl Snapshot {
    /// Stored retrieval capability for trusted transport adapters; never project its secrets to UI.
    #[must_use]
    pub fn attachment(&self) -> &Attachment {
        &self.photo
    }

    fn current(&self, store: &Store) -> Result<(), LookupError> {
        if store.owner_peer_id() != self.owner {
            return Err(LookupError::NotFound);
        }
        let current = stored(store, &self.peer, &self.group, &self.author, &self.id)?;
        if current != self.photo {
            return Err(LookupError::NotFound);
        }
        Ok(())
    }

    fn validate(&self, bytes: &[u8]) -> Result<(), LookupError> {
        if u64::try_from(bytes.len()).ok() != Some(self.photo.size)
            || dyapp_identity::sha256(bytes) != self.photo.plaintext_sha256
        {
            return Err(LookupError::Corrupted);
        }
        validate(bytes, self.photo.width, self.photo.height)
    }

    /// Read a sealed original without networking; a torn or foreign cache entry is a miss.
    ///
    /// # Errors
    /// Refuses stale/blocked selectors and authenticated cached bytes that fail photo validation.
    pub fn local(
        &self,
        store: &Store,
        dir: &Path,
        seal: &[u8; 32],
    ) -> Result<Option<Vec<u8>>, LookupError> {
        self.current(store)?;
        let Some(bytes) = originals::read(dir, seal, &self.photo) else {
            return Ok(None);
        };
        self.validate(&bytes)?;
        Ok(Some(bytes))
    }

    /// Validate a completed fetch and retain it best-effort under current settings.
    ///
    /// # Errors
    /// Refuses changed identity/message/block state or corrupt bytes before cache mutation.
    /// Cache setting, write and sweep failures do not discard a successfully fetched original.
    pub fn finish(
        &self,
        store: &Store,
        dir: &Path,
        seal: &[u8; 32],
        bytes: &[u8],
    ) -> Result<(), LookupError> {
        self.current(store)?;
        self.validate(bytes)?;
        if store.kept_originals_mib().is_ok_and(|limit| limit > 0) {
            if let Err(error) = originals::keep(dir, seal, &self.photo, bytes)
                .and_then(|()| store.sweep_originals(dir))
            {
                eprintln!("original not kept: {error}");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_size_must_equal_the_decrypted_length() {
        let bytes = include_bytes!("validate/three-by-two.png");
        let (photo, _) = crate::attachment::seal(bytes).unwrap();
        let mut snapshot = Snapshot {
            owner: String::new(),
            peer: String::new(),
            group: Vec::new(),
            author: String::new(),
            id: Vec::new(),
            photo,
        };
        for size in [0, u64::try_from(bytes.len()).unwrap() + 1] {
            snapshot.photo.size = size;
            assert!(matches!(
                snapshot.validate(bytes),
                Err(LookupError::Corrupted)
            ));
        }
        snapshot.photo.size = u64::try_from(bytes.len()).unwrap();
        snapshot.validate(bytes).unwrap();
    }
}
