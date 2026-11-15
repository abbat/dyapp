//! Contact confirmation against the current identity/network profile context.

use crate::profiles::Profiles;
use crate::session::Session;
use crate::store::Store;
use dyapp_profile::VerifiedProfile;
use std::sync::Arc;

/// Why a comparison cannot become a verified-contact mark.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The owner cannot confirm itself as a contact.
    #[error("ownPeer")]
    OwnPeer,
    /// The requested identifier is not an identity Peer ID.
    #[error("invalid Peer ID")]
    InvalidPeer,
    /// A node returned a profile for another owner.
    #[error("unknownPeer")]
    WrongPeer,
    /// Identity or network context changed after confirmation began.
    #[error("notConnected")]
    ContextChanged,
    /// The fetched profile cannot replace the held authority.
    #[error("staleProfile")]
    StaleProfile,
    /// An unpublished or deleted profile cannot receive a mark.
    #[error("unpublished")]
    Unpublished,
    /// Local state could not be read or committed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// The mark and durable device-change notice after observing the held profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct State {
    /// Whether the held profile's devices remain verified.
    pub verified: bool,
    /// Whether a device change still needs acknowledgement.
    pub new_device: bool,
}

/// A single confirmation's local owner, requested peer and context capability.
/// Not cloneable: a retry starts a fresh comparison against the current context.
#[derive(Debug)]
pub struct Ticket {
    context: Arc<()>,
    owner: String,
    peer: String,
}

/// Held ordinary profiles for one active identity/network context.
///
/// Keep this object under the same lock as context activation. Call [`Self::clear`]
/// when identity/network changes, including switching back to an earlier network.
/// The adapter fetches the highest observed signed profile after [`Self::begin`].
#[derive(Debug, Default)]
pub struct Context {
    context: Arc<()>,
    profiles: Profiles,
}

impl Context {
    /// The currently held ordinary profile authority.
    #[must_use]
    pub fn profiles(&self) -> &Profiles {
        &self.profiles
    }

    /// Admit a verified display/fetch result using the existing authority/version rules.
    ///
    /// # Errors
    /// Returns the rejection from [`Profiles::accept`].
    pub fn accept(&mut self, profile: VerifiedProfile) -> Result<&VerifiedProfile, crate::Error> {
        self.profiles.accept(profile)
    }

    /// Invalidate every admitted comparison and discard the previous context's profiles.
    pub fn clear(&mut self) {
        // A retained ticket keeps its old allocation alive, so pointer identity cannot recur.
        self.context = Arc::new(());
        self.profiles = Profiles::default();
    }

    /// Capture a one-use comparison before fetching the requested peer's latest profile.
    ///
    /// # Errors
    /// Rejects the local owner and invalid Peer IDs without changing local state.
    pub fn begin(&self, store: &Store, peer: &str) -> Result<Ticket, Error> {
        let owner = store.owner_peer_id();
        if owner == peer {
            return Err(Error::OwnPeer);
        }
        if dyapp_identity::safety::safety_number_for_peer_ids(owner, peer).is_none() {
            return Err(Error::InvalidPeer);
        }
        Ok(Ticket {
            context: Arc::clone(&self.context),
            owner: owner.to_owned(),
            peer: peer.to_owned(),
        })
    }

    /// Commit the user's comparison while the captured identity/network is still current.
    ///
    /// # Errors
    /// Rejects changed context/owner, wrong target, stale or nonordinary authority,
    /// unpublished/deleted profiles, or a failed local write. None writes a new mark.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "Confirmation consumes its one-use ticket even when validation fails"
    )]
    pub fn confirm(
        &mut self,
        store: &Store,
        ticket: Ticket,
        fetched: VerifiedProfile,
    ) -> Result<VerifiedProfile, Error> {
        if !Arc::ptr_eq(&self.context, &ticket.context) || store.owner_peer_id() != ticket.owner {
            return Err(Error::ContextChanged);
        }
        if fetched.peer_id != ticket.peer {
            return Err(Error::WrongPeer);
        }
        let held = self
            .profiles
            .accept(fetched)
            .map_err(|_| Error::StaleProfile)?;
        if held.profile.deleted || held.profile.version == 0 {
            return Err(Error::Unpublished);
        }
        store.set_verified(held).map_err(|error| match error {
            crate::store::Error::ProfileCompletionSuperseded => Error::StaleProfile,
            other => Error::Store(other),
        })?;
        Ok(held.clone())
    }

    /// Observe cached devices before reading the mark and notice; no cache means no mark.
    ///
    /// # Errors
    /// Returns a local state read/write error; device observation may update the notice.
    pub fn state(&self, store: &Store, peer: &str) -> Result<State, Error> {
        let verified = match self.profiles.get(peer) {
            Some(profile) => store.verified(profile)?,
            None => false,
        };
        Ok(State {
            verified,
            new_device: store.new_device(peer)?,
        })
    }
}

impl Session {
    /// Confirm through the shared identity/network context using this session's store.
    ///
    /// # Errors
    /// Returns the context, target, authority and storage failures of [`Context::confirm`].
    pub fn confirm_contact(
        &self,
        context: &mut Context,
        ticket: Ticket,
        fetched: VerifiedProfile,
    ) -> Result<VerifiedProfile, Error> {
        context.confirm(self.store(), ticket, fetched)
    }

    /// Observe the shared context's contact verification state.
    ///
    /// # Errors
    /// Returns the local state failures of [`Context::state`].
    pub fn contact_verification(&self, context: &Context, peer: &str) -> Result<State, Error> {
        context.state(self.store(), peer)
    }

    /// Remove the mark while preserving observed devices and any existing notice.
    ///
    /// # Errors
    /// Returns a local database error.
    pub fn unverify_contact(&self, peer: &str) -> Result<(), Error> {
        Ok(self.store().clear_verified(peer)?)
    }

    /// Acknowledge the notice without changing the mark or device memory.
    ///
    /// # Errors
    /// Returns a local database error.
    pub fn acknowledge_contact_devices(&self, peer: &str) -> Result<(), Error> {
        Ok(self.store().acknowledge_new_device(peer)?)
    }
}

#[cfg(test)]
mod tests;
