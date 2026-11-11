//! Convert the existing platform `KeyStore` capability for core chat bootstrap.

use crate::KeyStore;
use dyapp_client::identity_slots::{ChatKeyKind, ChatKeyStore, Slot};
use std::sync::Arc;
use zeroize::Zeroizing;

pub(crate) struct ChatWrapping(pub(crate) Arc<dyn KeyStore>);

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
