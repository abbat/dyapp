//! Read-only visibility admission shared by chat actions and confirmation targets.
use crate::store::{Conversation, Store};

/// Target identifiers read under the caller's identity/session lease, not mutation authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The actual admitted store owner.
    pub owner_peer_id: String,
    /// The selected known conversation peer.
    pub peer_id: String,
}

/// Existing native visible-chat failures, before writes or network work.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The selected peer is this store's owner.
    #[error("ownPeer")]
    OwnPeer,
    /// The selected peer is blocked.
    #[error("blocked")]
    Blocked,
    /// No conversation is known for the selected peer.
    #[error("unknownPeer")]
    UnknownPeer,
    /// Reading persistent state failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// Check the existing own/blocked/known-chat policy in that order.
///
/// # Errors
/// Returns the corresponding visible-chat refusal or the underlying storage failure.
pub fn check_visible(store: &Store, peer_id: &str) -> Result<(), Error> {
    if peer_id == store.owner_peer_id() {
        return Err(Error::OwnPeer);
    }
    if store.blocked(peer_id)? {
        return Err(Error::Blocked);
    }
    if store.conversation(peer_id)? == Conversation::None {
        return Err(Error::UnknownPeer);
    }
    Ok(())
}

/// Project a known visible target without changing conversation, history or owner state.
///
/// # Errors
/// Returns the same admission/storage failures as [`check_visible`].
pub fn target(store: &Store, peer_id: &str) -> Result<Target, Error> {
    check_visible(store, peer_id)?;
    Ok(Target {
        owner_peer_id: store.owner_peer_id().to_owned(),
        peer_id: peer_id.to_owned(),
    })
}

#[cfg(test)]
mod tests;
