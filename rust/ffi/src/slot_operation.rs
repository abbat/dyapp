//! Opaque common native ownership of an admitted core identity operation.

use crate::{IdentityKey, MobileSlot};
use std::sync::Arc;

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
