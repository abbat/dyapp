//! Convert the existing platform `KeyStore` capability for core chat bootstrap.

use crate::slot_keys::SlotKeys;
use crate::{IdentityKey, KeyStore};
use dyapp_client::identity_slots::{ChatKeyKind, ChatKeyStore, Slot};
use dyapp_identity::Identity;
use std::sync::Arc;
use zeroize::Zeroizing;

pub(crate) struct ChatWrapping(pub(crate) Arc<dyn KeyStore>);

impl ChatKeyStore for SlotKeys {
    fn wrap(&self, slot: &Slot, _: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        let key = Arc::new(IdentityKey {
            identity: Identity::from_secret(secret),
        });
        self.0
            .wrap_existing(slot.id.clone(), key)
            .map_err(|error| error.to_string())
    }

    fn unwrap(
        &self,
        slot: &Slot,
        _: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        self.0
            .open_wrapped(slot.id.clone(), wrapped.to_vec())
            .map_err(|error| error.to_string())?
            .map(|key| key.identity.secret())
            .ok_or_else(|| "The slot wrapping credential is missing".to_owned())
    }
}

impl ChatKeyStore for ChatWrapping {
    fn wrap(&self, _: &Slot, _: ChatKeyKind, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
        self.0
            .wrap(secret.to_vec())
            .map_err(|error| error.to_string())
    }

    fn unwrap(
        &self,
        _: &Slot,
        _: ChatKeyKind,
        wrapped: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        let bytes = Zeroizing::new(
            self.0
                .unwrap(wrapped.to_vec())
                .map_err(|error| error.to_string())?,
        );
        let secret = <[u8; 32]>::try_from(bytes.as_slice())
            .map_err(|_| "Unwrapped chat key is not 32 bytes")?;
        Ok(Zeroizing::new(secret))
    }
}
