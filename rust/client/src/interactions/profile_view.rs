//! One explicit profile opening, shared by native and webview adapters.
use crate::contact_verification::{Context, Generation};
use crate::store::Store;
use std::sync::atomic::{AtomicU8, Ordering};

const PENDING: u8 = 0;
const RETIRED: u8 = 1;
const ADMITTED: u8 = 2;

/// Why an explicit opening cannot admit a new private disclosure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The captured identity or network has been replaced.
    #[error("notConnected")]
    ContextChanged,
    /// Only canonical other-profile identifiers are allowed.
    #[error("invalidPeer")]
    InvalidPeer,
    /// The owner cannot view itself as another profile.
    #[error("ownPeer")]
    OwnPeer,
    /// Privacy and durable admission must be readable.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// Immutable recipient and context with one admission or retirement transition.
///
/// Construct at the explicit opening, not from a background fetch or a delayed send callback.
/// Capture the network under the same Context lock as admission; activation uses that lock too.
/// An admitted operation keeps that snapshot and may finish after the screen retires.
#[derive(Debug)]
pub struct Opening {
    owner: String,
    peer: String,
    generation: Generation,
    state: AtomicU8,
}

impl Opening {
    /// Capture an opening without reading storage or issuing network traffic.
    #[must_use]
    pub fn new(context: &Context, owner: String, peer: String) -> Self {
        Self::from_generation(context.generation(), owner, peer)
    }

    /// Bind a generation captured by a memory-only observer before scheduling backend work.
    #[must_use]
    pub fn from_generation(generation: Generation, owner: String, peer: String) -> Self {
        Self {
            owner,
            peer,
            generation,
            state: AtomicU8::new(PENDING),
        }
    }

    /// The original recipient; never substitute a later route's selected profile.
    #[must_use]
    pub fn peer(&self) -> &str {
        &self.peer
    }

    /// Prevent work which has not been admitted; do not cancel an admitted original operation.
    pub fn retire(&self) {
        let _ = self
            .state
            .compare_exchange(PENDING, RETIRED, Ordering::AcqRel, Ordering::Acquire);
    }

    /// Admit this opening at most once, using the shared durable preference/day policy.
    /// A false result is a retired, consumed or deliberately suppressed attempt, not a queued event.
    /// The final event staging repeats policy; admission itself writes no event or day.
    ///
    /// # Errors
    /// Rejects changed context/owner, invalid/own targets and unavailable durable policy.
    pub fn admit_at(&self, context: &Context, store: &Store, wall_ms: u64) -> Result<bool, Error> {
        if self.state.load(Ordering::Acquire) != PENDING {
            return Ok(false);
        }
        if !context.is_current(&self.generation) || store.owner_peer_id() != self.owner {
            return Err(Error::ContextChanged);
        }
        crate::private_peer::check(store, &self.peer).map_err(|error| match error {
            crate::private_peer::TargetError::Invalid => Error::InvalidPeer,
            crate::private_peer::TargetError::Own => Error::OwnPeer,
        })?;
        if self
            .state
            .compare_exchange(PENDING, ADMITTED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Ok(false);
        }
        Ok(store.profile_view_due_at(&self.peer, wall_ms)?)
    }
}

#[cfg(test)]
mod tests;
