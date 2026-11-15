//! Stored photo identity shared by native and desktop original retrieval.

use super::Attachment;
use crate::store::{self, Store};

/// Why a stored original selector cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    /// A blocked peer cannot expose even a locally retained original.
    #[error("blocked")]
    Blocked,
    /// No current photo matches the complete peer/group/author/message identity.
    #[error("notFound")]
    NotFound,
    /// Storage or descriptor consistency validation failed.
    #[error(transparent)]
    Store(#[from] store::Error),
}

/// Resolve only a current stored descriptor, never one supplied by the caller.
///
/// Malformed or stale selectors cannot bypass block admission or select a photo in another group.
/// This does not read original bytes, fetch media or change codec/download policy.
///
/// # Errors
/// Returns blocked, missing-photo, storage or stored-descriptor validation failures.
pub fn stored(
    store: &Store,
    peer_id: &str,
    group_id: &[u8],
    author: &str,
    id: &[u8],
) -> Result<Attachment, LookupError> {
    if store.blocked(peer_id)? {
        return Err(LookupError::Blocked);
    }
    store
        .current_chat_attachments(peer_id)?
        .into_iter()
        .find(|photo| {
            photo.key.group.as_slice() == group_id
                && photo.key.author == author
                && photo.key.id.as_slice() == id
        })
        .map(|photo| photo.message.attachment)
        .ok_or(LookupError::NotFound)
}

#[cfg(test)]
mod tests;
