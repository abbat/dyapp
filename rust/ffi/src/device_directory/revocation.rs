//! Opaque authority minted only by the shared core's verified mailbox/profile decision.
use super::{active_network, connect_at, failed, on_network_stack, MobileDeviceDirectory};
use crate::slot_operation::SlotWorker;
use crate::{CoreError, KeyStore};
use dyapp_client::identity_slots::ChatBinding;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

/// One confirmed original-device removal; contains no live operation lease.
#[derive(uniffi::Object)]
pub struct MobileRevokedDevice {
    slot: SlotWorker,
    wrapping: Arc<dyn KeyStore>,
    binding: ChatBinding,
    outcome: Mutex<Option<Result<Option<String>, String>>>,
}

#[uniffi::export]
impl MobileDeviceDirectory {
    /// Check an existing device after failed receiving without consuming its mailbox.
    /// Call off the UI thread; only a freshly confirmed revocation creates cleanup authority.
    ///
    /// # Errors
    /// Preserves changed credentials, unavailable network and unconfirmed mailbox/profile errors.
    pub fn revoked_device(&self) -> Result<Option<Arc<MobileRevokedDevice>>, CoreError> {
        on_network_stack(|| {
            let binding = {
                let admitted = self.open()?;
                let network = active_network(&admitted.operation.slot().dir)?;
                let confirmed = self.runtime.block_on(async {
                    let mut node = connect_at(&admitted.operation.slot().dir, &network).await?;
                    admitted
                        .session
                        .probe_mailbox_revocation(&mut node, admitted.operation.device())
                        .await
                        .map_err(failed)
                })?;
                confirmed.then(|| admitted.operation.binding())
            };
            Ok(binding.map(|binding| {
                Arc::new(MobileRevokedDevice {
                    slot: self.slot.clone(),
                    wrapping: Arc::clone(&self.wrapping),
                    binding,
                    outcome: Mutex::new(None),
                })
            }))
        })
    }
}

#[uniffi::export]
impl MobileRevokedDevice {
    /// Remove the originally confirmed slot; repeated calls retain the first terminal outcome.
    /// The optional detail reports visible completion with unconfirmed durability.
    ///
    /// # Errors
    /// Refuses replaced context and retains partial-cleanup failures without implicit retry.
    pub fn complete(&self) -> Result<Option<String>, CoreError> {
        let mut outcome = self.outcome.lock().map_err(failed)?;
        outcome
            .get_or_insert_with(|| {
                self.slot
                    .remove_bound_chat(&self.binding, Arc::clone(&self.wrapping))
                    .map_err(|error| error.to_string())
            })
            .clone()
            .map_err(CoreError::Failed)
    }
}
