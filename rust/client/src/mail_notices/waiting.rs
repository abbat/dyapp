//! Identity-bound waiting snapshots and notice admission, independent of OS posting.
use super::{content, Preview};
use crate::store::{ChatRow, NoticeContent, Store};

/// Notification snapshot or decision failure. Adapters keep failures quiet without exposing content.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Stored counts, conversation or privacy metadata could not be read.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// Snapshots belong to different identities.
    #[error("Identity changed")]
    IdentityChanged,
    /// Counts cannot be represented without wrapping.
    #[error("notification count overflow")]
    CountOverflow,
}

/// One identity's eligible unread rows and unanswered likes. Private fields prevent adapter edits.
#[derive(Clone)]
pub struct Waiting {
    owner: String,
    rows: Vec<ChatRow>,
    count: usize,
}
impl Waiting {
    fn new(owner: String, rows: Vec<ChatRow>, likes: usize) -> Result<Self, Error> {
        let count = rows.iter().try_fold(likes, |count, row| {
            count
                .checked_add(row.unread_count)
                .ok_or(Error::CountOverflow)
        })?;
        Ok(Self { owner, rows, count })
    }
}

/// An admitted local notice; no sender means the platform's fixed new-message text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// Allowed identity selector for a local name resolver; absent for fixed content.
    pub peer_id: Option<String>,
    /// Allowed current body, never a media descriptor or private pack metadata.
    pub preview: Preview,
}

/// Capture rows and unanswered likes after excluding blocked/muted peers, preserving core order.
/// A read unanswered request is not new unread mail; this is not the UI tab badge.
///
/// # Errors
/// Returns store failures or count overflow; no partial snapshot is returned.
pub fn capture(store: &Store) -> Result<Waiting, Error> {
    let quiet = |peer: &str| -> Result<bool, crate::store::Error> {
        Ok(store.blocked(peer)? || store.muted(peer)?)
    };
    let mut rows = Vec::new();
    for row in store.chat_rows()? {
        if !quiet(&row.peer_id)? {
            rows.push(row);
        }
    }
    let mut likes = 0usize;
    for peer in store.liked_by()? {
        if !quiet(&peer)? {
            likes = likes.checked_add(1).ok_or(Error::CountOverflow)?;
        }
    }
    Waiting::new(store.owner_peer_id().into(), rows, likes)
}

/// Decide using two same-identity snapshots and validated current notification preferences.
/// Disabled or non-increasing total means no notice. Otherwise fixed content remains available
/// even for a like/request; only eligible open-chat content may add sender/text.
/// Name lookup, localized fixed/sticker labels, escaping, foreground and lock-screen handling stay
/// in the platform adapter. The adapter retains identity admission across the actual mail pass.
///
/// # Errors
/// Refuses mixed owners before using counts or content.
pub fn decide(
    before: &Waiting,
    after: &Waiting,
    enabled: bool,
    mode: NoticeContent,
) -> Result<Option<Notice>, Error> {
    if before.owner != after.owner {
        return Err(Error::IdentityChanged);
    }
    if !enabled || after.count <= before.count {
        return Ok(None);
    }
    let selected = content(mode, &before.rows, &after.rows);
    Ok(Some(selected.map_or(
        Notice {
            peer_id: None,
            preview: Preview::Empty,
        },
        |selected| Notice {
            peer_id: Some(selected.peer_id.into()),
            preview: selected.preview,
        },
    )))
}

#[cfg(test)]
mod tests;
