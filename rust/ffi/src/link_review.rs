//! Idle owner-review handles retain no session, key or operation lease.
mod error;
mod fields;
mod view;
pub use error::LinkReviewError;
pub use fields::{LinkReviewChoice, LinkReviewField, LinkReviewSource};
pub use view::{LinkReviewPhoto, LinkReviewProfile, LinkReviewSaved, LinkReviewSnapshot};

use crate::profile_network::{active_network, connect, on_network_stack};
use crate::slot_operation::SlotWorker;
use crate::{CoreError, KeyStore, MobileIdentityRegistry};
use dyapp_client::{
    identity_slots::{ChatBinding, ChatOperation},
    publish,
    session::Session,
    store::Store,
};
use dyapp_identity::{network::Network, SignedRecord};
use error::failed;
use std::sync::Arc;

struct Owner {
    slot: SlotWorker,
    wrapping: Arc<dyn KeyStore>,
}
// Field order keeps admission alive until the session and database have closed.
struct Admitted {
    session: Session,
    operation: ChatOperation,
}
impl Owner {
    fn open(&self) -> Result<Admitted, LinkReviewError> {
        let operation = self
            .slot
            .existing_chat_operation(Arc::clone(&self.wrapping))?;
        let store = Store::open_existing(
            &operation.slot().dir.join("client.db"),
            operation.identity(),
        )
        .map_err(failed)?;
        let session = Session::open(store, operation.state_key()).map_err(failed)?;
        Ok(Admitted { session, operation })
    }
}

/// Common Android/iOS recovery preparation, bound to one explicit existing slot and owner.
#[derive(uniffi::Object)]
pub struct MobileLinkRecovery {
    owner: Arc<Owner>,
}

#[uniffi::export]
impl MobileLinkRecovery {
    /// Bind coordinates and existing credential access; no admission, bootstrap or network work.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
        store: Arc<dyn KeyStore>,
    ) -> Arc<Self> {
        Arc::new(Self {
            owner: Arc::new(Owner {
                slot: SlotWorker::new(registry, id, expected_peer),
                wrapping: store,
            }),
        })
    }

    /// Fetch an immutable comparison off the UI thread; release all leases before owner choice.
    ///
    /// # Errors
    /// Returns existing admission, credential, pending-link, network or verification failures.
    pub fn prepare(&self) -> Result<Arc<ProfileAdoptionReview>, LinkReviewError> {
        link_network(|runtime| {
            let admitted = self.owner.open()?;
            let certificate = admitted.session.certificate().map_err(failed)?;
            let network = active_network(&admitted.operation.slot().dir)?;
            let review = runtime.block_on(async {
                let mut node = connect(&network).await?;
                publish::review_adoption(
                    admitted.session.store(),
                    &mut node,
                    admitted.operation.identity(),
                )
                .await
                .map_err(LinkReviewError::from)
            })?;
            Ok(Arc::new(ProfileAdoptionReview {
                owner: Arc::clone(&self.owner),
                certificate,
                admission: admitted.operation.binding(),
                network,
                review,
            }))
        })
    }
}

/// Opaque original review, independent of the current UI selection and free of held credentials.
#[derive(uniffi::Object)]
pub struct ProfileAdoptionReview {
    owner: Arc<Owner>,
    certificate: SignedRecord,
    admission: ChatBinding,
    network: Network,
    review: publish::AdoptionReview,
}

#[uniffi::export]
impl ProfileAdoptionReview {
    /// Complete values for presentation; neither these records nor UI-supplied values authorize writes.
    #[must_use]
    pub fn snapshot(&self) -> LinkReviewSnapshot {
        LinkReviewSnapshot {
            local: self.review.local_profile().into(),
            remote: self.review.remote_profile().into(),
            differences: self
                .review
                .differences()
                .into_iter()
                .map(Into::into)
                .collect(),
            hidden: self.review.hidden(),
        }
    }

    /// Reopen the original session and save explicit choices through shared core confirmation.
    /// Public profile publication and unfinished link key packages remain separate operations.
    ///
    /// # Errors
    /// Returns typed invalid selections/profile, changed context/device, unlisted-device and
    /// backend errors. A failed retry can follow an already successful local save; refresh review.
    pub fn confirm(
        &self,
        choices: Vec<LinkReviewChoice>,
    ) -> Result<LinkReviewSaved, LinkReviewError> {
        let choices: Vec<_> = choices.into_iter().map(Into::into).collect();
        self.review
            .select_fields(&choices)
            .map_err(publish::AdoptionConfirmError::from)?;
        link_network(|runtime| {
            let admitted = self.owner.open()?;
            self.admit(&admitted)?;
            runtime.block_on(async {
                let mut node = connect(&self.network).await?;
                publish::confirm_adoption_review(
                    &admitted.session,
                    &mut node,
                    admitted.operation.identity(),
                    &self.review,
                    &choices,
                )
                .await
                .map(|outcome| LinkReviewSaved::from(&outcome))
                .map_err(LinkReviewError::from)
            })
        })
    }

    /// Resume the original pending link through shared core, including key packages and fallback.
    /// This is separate from field confirmation; it never publishes chosen editable values.
    /// Reopen the original existing session for each retry, retaining admission through completion.
    /// Returns false when no pending link remains, without connecting to the network.
    ///
    /// # Errors
    /// Refuses a replacement slot/device/network/link, ambiguous profile or private-state import,
    /// revoked authorization, and backend failures. Earlier steps may already have succeeded.
    pub fn resume_link(&self) -> Result<bool, LinkReviewError> {
        link_network(|runtime| {
            let admitted = self.owner.open()?;
            self.admit_device(&admitted)?;
            let Some(pending) = admitted.session.store().link_pending().map_err(failed)? else {
                return Ok(false);
            };
            if pending != self.review.pending_link_state() {
                return Err(LinkReviewError::ReviewRequired);
            }
            self.review
                .remember_device_membership(&admitted.session, admitted.operation.identity())
                .map_err(LinkReviewError::from)?;
            let Admitted { session, operation } = admitted;
            // The moved session lives in the awaited future; its admission outlives that future.
            runtime.block_on(async {
                let mut node = connect(&self.network).await?;
                session
                    .resume_link(
                        &mut node,
                        operation.identity(),
                        operation.device(),
                        crate::chats::KEY_PACKAGES,
                        operation.state_key(),
                        dyapp_client::now(),
                    )
                    .await
                    .map_err(LinkReviewError::from)
            })
        })
    }
}
impl ProfileAdoptionReview {
    fn admit(&self, admitted: &Admitted) -> Result<(), LinkReviewError> {
        self.admit_device(admitted)?;
        if !self
            .review
            .is_current(admitted.session.store(), admitted.operation.identity())
            .map_err(failed)?
        {
            return Err(LinkReviewError::ReviewRequired);
        }
        Ok(())
    }

    fn admit_device(&self, admitted: &Admitted) -> Result<(), LinkReviewError> {
        if admitted.operation.binding() != self.admission
            || admitted.session.certificate().map_err(failed)? != self.certificate
            || active_network(&admitted.operation.slot().dir)? != self.network
        {
            return Err(LinkReviewError::ReviewRequired);
        }
        Ok(())
    }
}

fn link_network<T: Send>(
    work: impl FnOnce(&tokio::runtime::Runtime) -> Result<T, LinkReviewError> + Send,
) -> Result<T, LinkReviewError> {
    on_network_stack(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(work(&runtime))
    })?
}

#[cfg(test)]
mod tests;
