//! Identity-bound private notes shared by native and desktop adapters.
use crate::{
    chat::{valid_note, PeerChange},
    session::Session,
    store::Store,
    Node,
};

/// Private-note operations distinguish target/text errors from backend failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The supplied value is not a peer ID.
    #[error("invalidNote")]
    InvalidPeer,
    /// An identity cannot annotate itself as a contact.
    #[error("invalidNote")]
    OwnPeer,
    /// Text exceeds the shared limit or contains unsupported text.
    #[error("invalidNote")]
    InvalidText,
    /// Persistent note state could not be read or updated.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    /// Own-device encrypted staging failed.
    #[error(transparent)]
    Session(#[from] crate::session::Error),
}

fn target(store: &Store, peer: &str) -> Result<(), Error> {
    crate::private_peer::check(store, peer).map_err(|error| match error {
        crate::private_peer::TargetError::Invalid => Error::InvalidPeer,
        crate::private_peer::TargetError::Own => Error::OwnPeer,
    })
}

/// Validate without fetching profiles; blocked contacts retain private note access.
///
/// # Errors
/// Returns a typed invalid target, own target or text error.
pub fn validate(store: &Store, peer: &str, note: &str) -> Result<(), Error> {
    target(store, peer)?;
    if !valid_note(note) {
        return Err(Error::InvalidText);
    }
    Ok(())
}

/// Read an absent note as empty, while preserving storage failure as an error.
///
/// # Errors
/// Returns a typed target or storage error.
pub fn read(store: &Store, peer: &str) -> Result<String, Error> {
    target(store, peer)?;
    Ok(store.contact_note(peer)?)
}

/// Save exact clean text, or clear with empty text, and stage own-device sync only.
///
/// # Errors
/// Returns a typed target, text, storage or own-device staging error.
pub async fn change(
    session: &mut Session,
    nodes: &mut [Node],
    peer: &str,
    note: String,
    seal_key: &[u8; 32],
) -> Result<(), Error> {
    validate(session.store(), peer, &note)?;
    session
        .change_peer(nodes, peer, PeerChange::Note(note), seal_key)
        .await?;
    Ok(())
}
