//! Thin native worker adapter for the shared receiver.
#[cfg(test)]
mod tests;

use crate::profile_network::{connect, on_network_stack};
use crate::slot_keys::SlotKeys;
use crate::{
    CoreError, MobileIdentityRegistry, MobileLinkFailure, MobileLinkPhase, MobileLinkReceipt,
};
use dyapp_client::identity_slots::{LinkReceiver, ReceivedLink};
use std::sync::Arc;

/// Source-bound receiving worker; core owns cancellation, admission and retained private state.
#[derive(uniffi::Object)]
pub struct MobileLinkReceiver {
    registry: Arc<MobileIdentityRegistry>,
    receiver: LinkReceiver<crate::MobileSlotSelection, MobileLinkFailure>,
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export]
impl MobileLinkReceiver {
    /// Capture a selected source and view generation without installing or retaining an idle lease.
    ///
    /// # Errors
    /// Returns source/credential admission, invitation or runtime construction failures.
    #[uniffi::constructor]
    #[expect(clippy::needless_pass_by_value, reason = "UniFFI lifts owned strings")]
    pub fn start(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
        wizard: bool,
    ) -> Result<Arc<Self>, CoreError> {
        let (invitation, generation) = registry.link_invitation(&id, &expected_peer, wizard)?;
        let receiver = LinkReceiver::new(invitation, generation).map_err(CoreError::Failed)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(Arc::new(Self {
            registry,
            receiver,
            runtime,
        }))
    }

    /// Original public QR text; show only while waiting or receiving.
    #[must_use]
    pub fn code(&self) -> String {
        self.receiver.code().text()
    }

    /// Original expiry in Unix seconds; Kotlin does not calculate the protocol lifetime.
    #[must_use]
    pub fn expires_at(&self) -> u64 {
        self.receiver.code().expires_at
    }

    /// Non-blocking progress, including while platform credentials are being written.
    #[must_use]
    pub fn phase(&self) -> MobileLinkPhase {
        self.receiver.phase().into()
    }

    /// Cancel before write admission; report the actual phase after that boundary.
    #[must_use]
    pub fn cancel(&self) -> MobileLinkPhase {
        self.receiver.cancel().into()
    }

    /// Original cached installation receipt, without another write or selection.
    ///
    /// # Errors
    /// Poisoned state requires explicit recovery instead of an implicit retry.
    pub fn result(&self) -> Result<Option<MobileLinkReceipt>, CoreError> {
        self.receiver
            .result()
            .map(|result| {
                result.map(|result| match result {
                    Ok(selection) => MobileLinkReceipt::Installed { selection },
                    Err(failure) => MobileLinkReceipt::Failed { failure },
                })
            })
            .map_err(CoreError::Failed)
    }

    /// Run off the UI thread. Own this call through actual completion, even after UI cancellation.
    /// No node-hosting or installation-owned node operation is started here.
    ///
    /// # Errors
    /// Returns network, stale-source, expiry or explicit interrupted-state failures.
    pub fn poll(&self) -> Result<MobileLinkPhase, CoreError> {
        on_network_stack(|| {
            self.runtime
                .block_on(
                    self.receiver.poll_with(
                        self.registry.link_keys(),
                        &self.registry.view,
                        dyapp_client::now(),
                        |network| async move {
                            connect(&network).await.map_err(|error| error.to_string())
                        },
                        |received| install(received, self.registry.link_keys()),
                    ),
                )
                .map(Into::into)
                .map_err(CoreError::Failed)
        })
    }
}

fn install(
    received: &mut ReceivedLink,
    keys: &SlotKeys,
) -> Result<crate::MobileSlotSelection, MobileLinkFailure> {
    received
        .install(keys)
        .map(|(slot, identity, durability)| {
            crate::slot_registry::selected(
                slot.clone(),
                dyapp_identity::Identity::from_secret(&identity.secret()),
                durability.clone(),
            )
        })
        .map_err(MobileLinkFailure::from)
}
