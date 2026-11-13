//! Opaque common native ownership of an admitted core identity operation.

use crate::{CoreError, IdentityKey, MobileIdentityRegistry, MobileSlot};
use std::sync::Arc;

/// Reusable admission coordinates; idle native handles retain no registry lock.
pub(crate) struct SlotWorker {
    registry: Arc<MobileIdentityRegistry>,
    id: String,
    expected_peer: String,
}

impl SlotWorker {
    pub(crate) fn new(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
    ) -> Self {
        Self {
            registry,
            id,
            expected_peer,
        }
    }

    pub(crate) fn operation(&self) -> Result<Arc<MobileIdentityOperation>, CoreError> {
        self.registry
            .operation(self.id.clone(), self.expected_peer.clone())
    }

    pub(crate) fn existing_chat_operation(
        &self,
        store: Arc<dyn crate::KeyStore>,
    ) -> Result<dyapp_client::identity_slots::ChatOperation, CoreError> {
        self.registry
            .existing_chat_operation(&self.id, &self.expected_peer, store)
    }
}

/// Hold through actual blocking-worker completion, including after UI cancellation.
///
/// Releasing this object permits registry mutations. Do not start a mutation while
/// retaining it, and do not retain it during installation-owned node operations.
#[derive(uniffi::Object)]
pub struct MobileIdentityOperation {
    pub(crate) operation: dyapp_client::identity_slots::IdentityOperation,
}

#[uniffi::export]
impl MobileIdentityOperation {
    /// Verified location belonging to this operation.
    #[must_use]
    pub fn slot(&self) -> MobileSlot {
        self.operation.slot().clone().into()
    }

    /// Opaque key; retaining the key alone does not retain the operation lease.
    #[must_use]
    pub fn key(&self) -> Arc<IdentityKey> {
        Arc::new(IdentityKey {
            identity: dyapp_identity::Identity::from_secret(&self.operation.identity().secret()),
        })
    }
}
