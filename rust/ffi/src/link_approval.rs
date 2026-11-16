//! Thin owner-side confirmation over the shared encrypted-transfer workflow.
#[cfg(test)]
mod tests;

use crate::profile_network::{connect, on_network_stack};
use crate::{CoreError, MobileIdentityRegistry};
use dyapp_client::link::{parse_input, CodeInputError};
use std::sync::Arc;

/// Pasted-code presentation status; validation performs no transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum MobileLinkCodeStatus {
    /// A well-formed code that has not expired.
    Valid,
    /// Malformed input, including an invalid zero expiry.
    Malformed,
    /// A well-formed code whose deadline has arrived.
    Expired,
}

/// Check before presenting the existing full-control confirmation.
///
/// Keeps code normalization and expiry policy in the core.
#[uniffi::export]
#[must_use]
#[expect(clippy::needless_pass_by_value, reason = "UniFFI lifts owned strings")]
pub fn device_link_code_status(text: String) -> MobileLinkCodeStatus {
    match parse_input(&text, dyapp_client::now()) {
        Ok(_) => MobileLinkCodeStatus::Valid,
        Err(CodeInputError::Bad) => MobileLinkCodeStatus::Malformed,
        Err(CodeInputError::Expired) => MobileLinkCodeStatus::Expired,
    }
}

/// Immutable target prepared for explicit owner confirmation, with no idle identity lease.
#[derive(uniffi::Object)]
pub struct MobileLinkApproval {
    registry: Arc<MobileIdentityRegistry>,
    sender: dyapp_client::identity_slots::LinkSender,
    runtime: tokio::runtime::Runtime,
}

#[uniffi::export]
impl MobileLinkApproval {
    /// Prepare this exact target, without exporting private state or contacting a node.
    ///
    /// # Errors
    /// Returns malformed/expired code, source, credential, view or runtime failures.
    #[uniffi::constructor]
    #[expect(clippy::needless_pass_by_value, reason = "UniFFI lifts owned strings")]
    pub fn prepare(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
        code: String,
    ) -> Result<Arc<Self>, CoreError> {
        let sender = registry.link_sender(&id, &expected_peer, &code)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(Arc::new(Self {
            registry,
            sender,
            runtime,
        }))
    }

    /// Exact normalized public target that the owner is being asked to approve.
    #[must_use]
    pub fn code(&self) -> String {
        self.sender.code().text()
    }

    /// Target's actual expiry; Kotlin does not calculate the lifetime.
    #[must_use]
    pub fn expires_at(&self) -> u64 {
        self.sender.code().expires_at
    }

    /// Call off the UI thread only after explicit full-control confirmation of this target.
    /// Retain the actual worker through completion after UI cancellation; no node is hosted.
    ///
    /// # Errors
    /// Rechecks original view/source/owner/expiry, then returns snapshot/network/delivery failures.
    /// Some encrypted parts can already be sent on network error; no rollback is promised.
    pub fn confirm_and_send(&self) -> Result<(), CoreError> {
        on_network_stack(|| {
            self.runtime
                .block_on(
                    self.sender.confirm_and_send_with(
                        self.registry.link_keys(),
                        &self.registry.view,
                        dyapp_client::now(),
                        |network| async move {
                            connect(&network).await.map_err(|error| error.to_string())
                        },
                    ),
                )
                .map_err(CoreError::Failed)
        })
    }
}
