//! Contact confirmation against the current identity/network profile context.

use crate::profiles::Profiles;
use crate::scores::Scores;
use crate::session::Session;
use crate::store::Store;
use dyapp_profile::VerifiedProfile;
use std::sync::Arc;

mod registry;
pub use registry::{resume_all, scores, shared};
mod fetch;
pub use fetch::{fetch, FetchError};

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

/// Opaque generation captured by search/display work; it grants no confirmation authority.
/// Retaining a clone prevents the allocation from being reused after invalidation.
#[derive(Clone, Debug)]
pub struct Generation(Arc<()>);

/// A single confirmation's local owner, requested peer and context capability.
/// Not cloneable: a retry starts a fresh comparison against the current context.
#[derive(Debug)]
pub struct Ticket {
    context: Generation,
    owner: String,
    peer: String,
}

/// Held ordinary profiles and selection health for one active identity/network context.
///
/// Keep this object under the same lock as context activation. Call [`Self::clear`]
/// when identity/network changes, including switching back to an earlier network.
/// The adapter fetches the highest observed signed profile after [`Self::begin`].
#[derive(Debug, Default)]
pub struct Context {
    generation: Arc<()>,
    profiles: Profiles,
    scores: Arc<tokio::sync::Mutex<Scores>>,
}

impl Context {
    /// Capture the current generation before starting search/display work.
    #[must_use]
    pub fn generation(&self) -> Generation {
        Generation(Arc::clone(&self.generation))
    }

    /// Whether work captured this context's current generation.
    #[must_use]
    pub fn is_current(&self, generation: &Generation) -> bool {
        Arc::ptr_eq(&self.generation, &generation.0)
    }

    /// The currently held ordinary profile authority.
    #[must_use]
    pub fn profiles(&self) -> &Profiles {
        &self.profiles
    }

    /// The persistent selection state of this identity/network context.
    #[must_use]
    pub fn scores(&self) -> Arc<tokio::sync::Mutex<Scores>> {
        Arc::clone(&self.scores)
    }

    /// Move open circuits to half-open after an observed platform resume.
    pub async fn resume(&self) {
        self.scores.lock().await.resume();
    }

    /// Admit a verified display/fetch result using the existing authority/version rules.
    ///
    /// # Errors
    /// Returns the rejection from [`Profiles::accept`].
    pub fn accept(&mut self, profile: VerifiedProfile) -> Result<&VerifiedProfile, crate::Error> {
        self.profiles.accept(profile)
    }

    /// Invalidate comparisons and discard the previous context's profiles and selection health.
    pub fn clear(&mut self) {
        // Retained generations/tickets prevent allocation reuse, so old work cannot become current.
        self.generation = Arc::new(());
        self.profiles = Profiles::default();
        self.scores = Arc::new(tokio::sync::Mutex::new(Scores::default()));
    }

    /// Capture a one-use comparison before fetching the requested peer's latest profile.
    ///
    /// # Errors
    /// Rejects the local owner and invalid Peer IDs without changing local state.
    pub fn begin(&self, store: &Store, peer: &str) -> Result<Ticket, Error> {
        target(store, peer)?;
        let owner = store.owner_peer_id();
        Ok(Ticket {
            context: self.generation(),
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
        if !self.is_current(&ticket.context) || store.owner_peer_id() != ticket.owner {
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
    /// Rejects invalid/own targets or local state failures; observation may update the notice.
    pub fn state(&self, store: &Store, peer: &str) -> Result<State, Error> {
        target(store, peer)?;
        let verified = match self.profiles.get(peer) {
            Some(profile) if !profile.profile.deleted && profile.profile.version != 0 => {
                store.verified(profile)?
            }
            _ => false,
        };
        Ok(State {
            verified,
            new_device: store.new_device(peer)?,
        })
    }
}

fn target(store: &Store, peer: &str) -> Result<(), Error> {
    crate::private_peer::check(store, peer).map_err(|error| match error {
        crate::private_peer::TargetError::Invalid => Error::InvalidPeer,
        crate::private_peer::TargetError::Own => Error::OwnPeer,
    })
}

/// Remove the mark without opening an MLS session, preserving device memory and notices.
///
/// # Errors
/// Rejects invalid/own targets before mutation and returns local database errors.
pub fn unverify(store: &Store, peer: &str) -> Result<(), Error> {
    target(store, peer)?;
    Ok(store.clear_verified(peer)?)
}

/// Acknowledge a device notice without changing trust or opening an MLS session.
///
/// # Errors
/// Rejects invalid/own targets before mutation and returns local database errors.
pub fn acknowledge_devices(store: &Store, peer: &str) -> Result<(), Error> {
    target(store, peer)?;
    Ok(store.acknowledge_new_device(peer)?)
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
    /// Rejects invalid/own targets and returns local database errors.
    pub fn unverify_contact(&self, peer: &str) -> Result<(), Error> {
        unverify(self.store(), peer)
    }

    /// Acknowledge the notice without changing the mark or device memory.
    ///
    /// # Errors
    /// Rejects invalid/own targets and returns local database errors.
    pub fn acknowledge_contact_devices(&self, peer: &str) -> Result<(), Error> {
        acknowledge_devices(self.store(), peer)
    }
}

#[cfg(test)]
mod tests;
