//! Platform credential callbacks for the common mobile identity registry.

use crate::{IdentityKey, WrapError};
use dyapp_client::identity_slots::{Credential, KeyReader, KeyRemover, KeyWriter, Slot};
use dyapp_identity::Identity;
use std::sync::Arc;

/// Platform credentials only; registry files and lifecycle belong to the Rust core.
#[uniffi::export(with_foreign)]
pub trait MobileSlotKeys: Send + Sync {
    /// Whether this slot's wrapping credential exists.
    ///
    /// # Errors
    /// Returns platform access failures, never a fabricated absence.
    fn contains(&self, slot_id: String) -> Result<bool, WrapError>;

    /// Open an existing wrapped blob using the slot's existing credential.
    ///
    /// # Errors
    /// Returns access or unwrap failures; None means confirmed missing credentials.
    fn open_wrapped(
        &self,
        slot_id: String,
        wrapped: Vec<u8>,
    ) -> Result<Option<Arc<IdentityKey>>, WrapError>;

    /// Wrap an opaque identity using a fresh slot credential; refuse an existing alias.
    ///
    /// # Errors
    /// Returns credential creation or wrapping failures without replacing existing credentials.
    fn wrap(&self, slot_id: String, key: Arc<IdentityKey>) -> Result<Vec<u8>, WrapError>;

    /// Erase this slot's wrapping credential after core confirmation and ownership checks.
    ///
    /// # Errors
    /// Returns credential erasure failures; core verifies absence independently.
    fn erase(&self, slot_id: String) -> Result<(), WrapError>;
}

impl KeyRemover for SlotKeys {
    fn credential_present(&self, slot: &Slot, role: Credential) -> Result<bool, String> {
        if role != Credential::Identity {
            return Err("Mobile storage has one wrapping credential per slot".into());
        }
        KeyReader::contains(self, slot)
    }

    fn erase_credential(&self, slot: &Slot, role: Credential) -> Result<(), String> {
        if role != Credential::Identity {
            return Err("Mobile storage has one wrapping credential per slot".into());
        }
        self.0
            .erase(slot.id.clone())
            .map_err(|error| error.to_string())
    }
}

pub(crate) struct SlotKeys(pub(crate) Arc<dyn MobileSlotKeys>);

impl KeyReader for SlotKeys {
    fn contains(&self, slot: &Slot) -> Result<bool, String> {
        self.0
            .contains(slot.id.clone())
            .map_err(|error| error.to_string())
    }

    fn open(&self, slot: &Slot, wrapped: Option<&[u8]>) -> Result<Option<Identity>, String> {
        let wrapped = wrapped.ok_or("Mobile registry requires wrapped key storage")?;
        self.0
            .open_wrapped(slot.id.clone(), wrapped.to_vec())
            .map(|key| key.map(|key| Identity::from_secret(&key.identity.secret())))
            .map_err(|error| error.to_string())
    }
}

impl KeyWriter for SlotKeys {
    fn store(&self, slot: &Slot, identity: &Identity) -> Result<Option<Vec<u8>>, String> {
        let key = Arc::new(IdentityKey {
            identity: Identity::from_secret(&identity.secret()),
        });
        self.0
            .wrap(slot.id.clone(), key)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}
