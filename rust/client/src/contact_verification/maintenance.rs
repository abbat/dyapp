//! Bounded idle-contact refresh policy shared by desktop and native workers.
use super::{Context, Generation};
use crate::store::{self, Store};
use dyapp_profile::VerifiedProfile;

/// Select one non-owner conversation in persisted group order for this minute's refresh.
///
/// Multiple groups for one peer retain their existing turns. No contact means no request.
///
/// # Errors
/// Returns catalog read failures or an unrepresentable catalog/index size.
pub fn refresh_target(store: &Store, now: u64) -> Result<Option<String>, store::Error> {
    let owner = store.owner_peer_id();
    let mut peers = store.known_groups()?;
    peers.retain(|(_, peer)| peer != owner);
    if peers.is_empty() {
        return Ok(None);
    }
    // Reduce before narrowing so the same clock selects the same turn on 32/64-bit clients.
    let count = u64::try_from(peers.len()).map_err(|_| store::Error::InvalidSetting)?;
    let turn = (now / 60)
        .checked_rem(count)
        .ok_or(store::Error::InvalidSetting)?;
    let turn = usize::try_from(turn).map_err(|_| store::Error::InvalidSetting)?;
    Ok(peers.get(turn).map(|(_, peer)| peer.clone()))
}

impl Context {
    /// Admit a requested contact readback only in the identity/network generation that began it.
    /// Owner authority must instead use `accept_own`, which also checks durable owner clocks.
    /// Returns false for an obsolete generation without changing held profiles.
    ///
    /// # Errors
    /// Rejects an owner target, another peer's reply or ordinary profile authority failures.
    pub fn accept_contact_if_current(
        &mut self,
        generation: &Generation,
        store: &Store,
        peer: &str,
        profile: VerifiedProfile,
    ) -> Result<bool, crate::Error> {
        if !self.is_current(generation) {
            return Ok(false);
        }
        if peer == store.owner_peer_id() {
            return Err(crate::Error::Input("ownPeer"));
        }
        if profile.peer_id != peer {
            return Err(crate::Error::Input("unknownPeer"));
        }
        self.profiles.accept(profile)?;
        Ok(true)
    }
}
