//! Canonical application targets; signed peer-change wire validation remains unchanged.
use crate::{chat::PeerChange, store::Store};

pub(crate) enum TargetError {
    Invalid,
    Own,
}

pub(crate) fn check(store: &Store, peer: &str) -> Result<(), TargetError> {
    if !PeerChange::Favorite(false).valid(peer) || peer.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(TargetError::Invalid);
    }
    if peer == store.owner_peer_id() {
        return Err(TargetError::Own);
    }
    Ok(())
}
