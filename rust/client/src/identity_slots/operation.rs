use super::{KeyReader, Registry, Resolution, Slot};
use dyapp_identity::Identity;
use std::fs::File;

/// Verified identity and shared registry lock retained for one complete operation.
///
/// Move this guard into blocking workers and retain it until their actual completion,
/// including after caller cancellation. Do not acquire a registry mutation lock while
/// holding it. Installation-owned node operations must not retain an identity lease.
#[must_use = "dropping the operation releases its identity mutation protection"]
pub struct IdentityOperation {
    slot: Slot,
    identity: Box<Identity>,
    _guard: File,
}

impl IdentityOperation {
    /// Verified existing identity location.
    #[must_use]
    pub fn slot(&self) -> &Slot {
        &self.slot
    }

    /// Existing identity key, valid for the guarded operation.
    #[must_use]
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

impl Registry {
    /// Admit an operation only after a listed ready key matches the expected Peer ID.
    /// The returned guard excludes cooperating create/select/remove mutations.
    ///
    /// # Errors
    /// Returns lock, inventory, lifecycle, key access or identity-mismatch failures.
    pub fn operation(
        &self,
        id: &str,
        expected_peer: &str,
        keys: &dyn KeyReader,
    ) -> Result<IdentityOperation, String> {
        let guard = self.shared_lock()?;
        let slot = super::list(&self.root, self.layout)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|slot| slot.id == id && slot.state == super::State::Ready)
            .ok_or("Identity slot is missing or not ready")?;
        let Resolution::Ready(slot, identity) = super::open_slot(slot, keys) else {
            return Err("Identity key is missing or unavailable".into());
        };
        if identity.peer_id() != expected_peer {
            return Err("The identity key changed".into());
        }
        Ok(IdentityOperation {
            slot,
            identity,
            _guard: guard,
        })
    }
}
