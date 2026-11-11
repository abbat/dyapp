//! Private favorite policy shared by native and desktop adapters.
use crate::{chat::PeerChange, session::Session, store::Store, Node};

/// Favorite operations distinguish invalid targets and policy refusal from backend failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The supplied value is not a peer ID.
    #[error("invalidName")]
    InvalidPeer,
    /// An identity cannot favorite itself.
    #[error("invalidName")]
    OwnPeer,
    /// Saving a blocked peer is refused; removing it remains allowed.
    #[error("blocked")]
    Blocked,
    /// Reading or updating persistent state failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// Own-device encrypted staging failed.
    #[error(transparent)]
    Session(#[from] crate::session::Error),
}

/// Validate a target against the identity bound to the store.
///
/// # Errors
/// Returns a typed target, block-policy or storage error.
pub fn validate(store: &Store, peer: &str, saving: bool) -> Result<(), Error> {
    crate::private_peer::check(store, peer).map_err(|error| match error {
        crate::private_peer::TargetError::Invalid => Error::InvalidPeer,
        crate::private_peer::TargetError::Own => Error::OwnPeer,
    })?;
    if saving && store.blocked(peer)? {
        return Err(Error::Blocked);
    }
    Ok(())
}

/// Read even a blocked favorite, allowing removal without a profile fetch.
///
/// # Errors
/// Returns a typed target or storage error.
pub fn state(store: &Store, peer: &str) -> Result<bool, Error> {
    validate(store, peer, false)?;
    Ok(store.favorite(peer)?)
}

/// Update locally and stage own-device sync without notifying the target peer.
///
/// # Errors
/// Returns a typed target, block-policy, storage or own-device staging error.
pub async fn change(
    session: &mut Session,
    nodes: &mut [Node],
    peer: &str,
    saving: bool,
    seal_key: &[u8; 32],
) -> Result<(), Error> {
    validate(session.store(), peer, saving)?;
    session
        .change_peer(nodes, peer, PeerChange::Favorite(saving), seal_key)
        .await?;
    Ok(())
}
