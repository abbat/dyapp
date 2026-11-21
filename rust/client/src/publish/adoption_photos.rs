//! Frozen review photo lookup and bounded decoding, shared by native presentations.
use super::{AdoptionReview, AdoptionSource};
use crate::{photo, store::Store, Node};

/// A comparison thumbnail could not be read; the offered photo choice remains unchanged.
#[derive(Debug, thiserror::Error)]
pub enum AdoptionPhotoError {
    /// The index does not belong to the selected frozen source.
    #[error("photo is not offered by this comparison")]
    Index,
    /// The offered thumbnail is not a content hash.
    #[error("invalid comparison photo reference")]
    Reference,
    /// Reading the original identity's stored blob failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// No answering node supplied a valid bounded thumbnail.
    #[error("comparison thumbnail is unavailable")]
    Unavailable,
}

impl AdoptionReview {
    /// Read a verified bounded thumbnail from the original store without network access or writes.
    /// Missing or invalid cached bytes return `None`; they never remove the offered photo.
    ///
    /// # Errors
    /// Rejects out-of-range/malformed frozen references and propagates storage errors.
    pub fn cached_thumbnail(
        &self,
        store: &Store,
        source: AdoptionSource,
        index: usize,
    ) -> Result<Option<photo::Rgba>, AdoptionPhotoError> {
        let hash = self.thumbnail_hash(source, index)?;
        Ok(store
            .profile_photo(hash)?
            .and_then(|bytes| decode_thumbnail(hash, &bytes)))
    }

    /// Read the exact offered thumbnail from answering nodes without caching on disk or publishing.
    /// The adapter admits the original identity/network before supplying these connections.
    ///
    /// # Errors
    /// Rejects invalid frozen references or returns unavailable after every node fails/refuses it.
    pub async fn fetch_thumbnail(
        &self,
        nodes: &mut [Node],
        source: AdoptionSource,
        index: usize,
    ) -> Result<photo::Rgba, AdoptionPhotoError> {
        let hash = self.thumbnail_hash(source, index)?;
        for node in nodes {
            let Ok(bytes) = node.get_media(hash).await else {
                continue;
            };
            if let Some(decoded) = decode_thumbnail(hash, &bytes) {
                return Ok(decoded);
            }
        }
        Err(AdoptionPhotoError::Unavailable)
    }

    fn thumbnail_hash(
        &self,
        source: AdoptionSource,
        index: usize,
    ) -> Result<&[u8; 32], AdoptionPhotoError> {
        let profile = match source {
            AdoptionSource::Local => self.local_profile(),
            AdoptionSource::Remote => self.remote_profile(),
        };
        profile
            .photos
            .get(index)
            .ok_or(AdoptionPhotoError::Index)?
            .thumbnail
            .as_slice()
            .try_into()
            .map_err(|_| AdoptionPhotoError::Reference)
    }
}

fn decode_thumbnail(hash: &[u8; 32], bytes: &[u8]) -> Option<photo::Rgba> {
    if dyapp_identity::sha256(bytes) != *hash {
        return None;
    }
    photo::check_preview(bytes).ok()
}

#[cfg(test)]
mod tests;
