//! Thin native ownership for common-core own-device directory operations.
mod revocation;
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::slot_operation::SlotWorker;
use crate::{CoreError, KeyStore, MobileIdentityRegistry};
use dyapp_client::{
    device_directory,
    identity_slots::{ChatBinding, ChatOperation},
    publish,
    session::Session,
    store::Store,
};
pub use revocation::MobileRevokedDevice;
use std::sync::Arc;

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// Public device selector and its current-device marker.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MobileOwnDevice {
    /// Public device key for explicit revocation, never a secret.
    pub key: Vec<u8>,
    /// Whether this is the admitted client's existing device.
    pub current: bool,
}

/// Idle handle retains original opaque coordinates, not keys, sessions or registry leases.
#[derive(uniffi::Object)]
pub struct MobileDeviceDirectory {
    slot: SlotWorker,
    wrapping: Arc<dyn KeyStore>,
    binding: ChatBinding,
    runtime: tokio::runtime::Runtime,
}

// Drop the session/database before releasing its serialized identity admission.
struct Admitted {
    session: Session,
    operation: ChatOperation,
}

fn existing(slot: &SlotWorker, wrapping: Arc<dyn KeyStore>) -> Result<Admitted, CoreError> {
    let operation = slot.existing_chat_operation(wrapping)?;
    let store = Store::open_existing(
        &operation.slot().dir.join("client.db"),
        operation.identity(),
    )
    .map_err(failed)?;
    let session = Session::open(store, operation.state_key()).map_err(failed)?;
    Ok(Admitted { session, operation })
}

#[uniffi::export]
impl MobileDeviceDirectory {
    /// Bind an already initialized slot/device without creating missing credentials or networking.
    ///
    /// # Errors
    /// Returns slot, credential, sealed-session, database or runtime failures.
    #[uniffi::constructor]
    pub fn new(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
        store: Arc<dyn KeyStore>,
    ) -> Result<Arc<Self>, CoreError> {
        let slot = SlotWorker::new(registry, id, expected_peer);
        let binding = existing(&slot, Arc::clone(&store))?.operation.binding();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(failed)?;
        Ok(Arc::new(Self {
            slot,
            wrapping: store,
            binding,
            runtime,
        }))
    }

    /// Read the shared core projection off the UI thread; offline live reads keep cached rows.
    /// Neither verified readback nor fallback replaces editable fields.
    ///
    /// # Errors
    /// Returns changed ownership, missing/replaced credentials or unreadable local state.
    pub fn devices(&self, live: bool) -> Result<Vec<MobileOwnDevice>, CoreError> {
        on_network_stack(|| {
            let admitted = self.open()?;
            let draft = admitted
                .session
                .store()
                .profile_draft()
                .map_err(failed)?
                .unwrap_or_default();
            let remote = self.runtime.block_on(async {
                let network = live
                    .then(|| active_network(&admitted.operation.slot().dir).ok())
                    .flatten()?;
                let mut node = connect(&network).await.ok()?;
                let scores =
                    dyapp_client::contact_verification::scores(&admitted.operation.slot().dir)
                        .ok()?;
                let mut scores = scores.lock().await;
                publish::newest_with_scores(&mut node, admitted.operation.identity(), &mut scores)
                    .await
                    .ok()
                    .flatten()
            });
            Ok(device_directory::rows(
                &draft,
                remote.as_ref(),
                &admitted.operation.device().public_key(),
            )
            .into_iter()
            .map(|device| MobileOwnDevice {
                key: device.key,
                current: device.this,
            })
            .collect())
        })
    }

    /// Revoke only the explicitly confirmed public target using the existing core workflow.
    /// Call off the UI thread and retain completion after presentation cancellation.
    ///
    /// # Errors
    /// Returns invalid target, changed admission or core publication/network/storage failures.
    /// Mailbox closure or publication may precede an error; no rollback or automatic retry is claimed.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI lifts owned byte arrays"
    )]
    pub fn revoke(&self, device_key: Vec<u8>) -> Result<(), CoreError> {
        if device_key.len() != 32 {
            return Err(CoreError::Failed("invalidDevice".into()));
        }
        on_network_stack(|| {
            let admitted = self.open()?;
            let store = admitted.session.store();
            let draft = store.profile_draft().map_err(failed)?.unwrap_or_default();
            let network = active_network(&admitted.operation.slot().dir)?;
            self.runtime.block_on(async {
                let mut node = connect(&network).await?;
                let scores =
                    dyapp_client::contact_verification::scores(&admitted.operation.slot().dir)
                        .map_err(|error| failed(error.to_string()))?;
                let mut scores = scores.lock().await;
                publish::revoke_device_with_scores(
                    store,
                    &mut node,
                    admitted.operation.identity(),
                    &draft,
                    &device_key,
                    &mut scores,
                )
                .await
                .map_err(failed)?;
                Ok(())
            })
        })
    }
}

impl MobileDeviceDirectory {
    fn open(&self) -> Result<Admitted, CoreError> {
        let admitted = existing(&self.slot, Arc::clone(&self.wrapping))?;
        if admitted.operation.binding() != self.binding {
            return Err(CoreError::Failed("Device context changed".into()));
        }
        Ok(admitted)
    }
}

#[cfg(test)]
mod tests;
