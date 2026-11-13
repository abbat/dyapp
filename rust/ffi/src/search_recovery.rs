//! Thin native handles for shared owner-confirmed search recovery.
use crate::slot_operation::SlotWorker;
use crate::{CoreError, KeyStore, MobileIdentityRegistry};
use dyapp_client::{
    identity_slots::{ChatBinding, ChatOperation},
    store::{self, Store},
};
use std::sync::{Arc, Mutex};

/// Why a native search recovery review cannot proceed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SearchRecoveryError {
    /// This review was explicitly closed.
    #[error("search recovery review is closed")]
    Closed,
    /// The reviewed state or original credentials changed.
    #[error("search recovery review changed")]
    Changed,
    /// Missing draft or conflicting visible intent needs a separate owner decision.
    #[error("search withdrawal requires owner review")]
    ReviewRequired,
    /// A linked-device recovery must finish first.
    #[error("linked-device recovery is pending")]
    LinkPending,
    /// No fresh search version can be represented.
    #[error("search profile version cannot advance")]
    VersionExhausted,
    /// The original owner no longer matches.
    #[error("search recovery belongs to another owner")]
    WrongOwner,
    /// Existing-only admission or storage failed.
    #[error("{reason}")]
    Failed {
        /// Backend diagnostic, without key material.
        reason: String,
    },
}
impl From<store::Error> for SearchRecoveryError {
    fn from(error: store::Error) -> Self {
        match error {
            store::Error::SearchWithdrawalRecoveryChanged => Self::Changed,
            store::Error::SearchWithdrawalReviewRequired => Self::ReviewRequired,
            store::Error::LinkStateReviewRequired => Self::LinkPending,
            store::Error::ProfileVersionExhausted => Self::VersionExhausted,
            store::Error::WrongIdentity => Self::WrongOwner,
            error => Self::Failed {
                reason: error.to_string(),
            },
        }
    }
}
impl From<CoreError> for SearchRecoveryError {
    fn from(error: CoreError) -> Self {
        Self::Failed {
            reason: error.to_string(),
        }
    }
}
fn poisoned<T>(_: std::sync::PoisonError<T>) -> SearchRecoveryError {
    SearchRecoveryError::Failed {
        reason: "search recovery is unavailable".into(),
    }
}

/// Current local state after confirmation; acknowledgement is not global removal proof.
#[derive(Debug, PartialEq, Eq, uniffi::Record)]
pub struct SearchRecoverySaved {
    /// Current local draft version.
    pub version: u64,
    /// Current local visibility intent.
    pub hidden: bool,
    /// Whether signed search publication remains queued for delivery.
    pub search_pending: bool,
}
impl From<store::SearchVisibility> for SearchRecoverySaved {
    fn from(state: store::SearchVisibility) -> Self {
        Self {
            version: state.version,
            hidden: state.hidden,
            search_pending: state.search_pending,
        }
    }
}
struct Owner {
    slot: SlotWorker,
    wrapping: Arc<dyn KeyStore>,
}
// Drop the database before releasing admission.
struct Admitted {
    store: Store,
    operation: ChatOperation,
}
impl Owner {
    fn open(&self) -> Result<Admitted, SearchRecoveryError> {
        let operation = self
            .slot
            .existing_chat_operation(Arc::clone(&self.wrapping))?;
        let store = Store::open_existing(
            &operation.slot().dir.join("client.db"),
            operation.identity(),
        )?;
        Ok(Admitted { store, operation })
    }
}

/// Existing-slot recovery preparation shared by Android and iOS; idle instances hold no lease.
#[derive(uniffi::Object)]
pub struct MobileSearchRecovery {
    owner: Arc<Owner>,
}
#[uniffi::export]
impl MobileSearchRecovery {
    /// Bind the explicit original slot; no admission, credentials or database are opened here.
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
    /// Prepare on a worker, releasing original-slot admission before returning for owner choice.
    ///
    /// # Errors
    /// Returns existing-only admission, owner-review or storage failures.
    pub fn prepare(&self) -> Result<Option<Arc<SearchWithdrawalReview>>, SearchRecoveryError> {
        let admitted = self.owner.open()?;
        let Some(plan) = admitted.store.prepare_search_withdrawal_recovery()? else {
            return Ok(None);
        };
        Ok(Some(Arc::new(SearchWithdrawalReview {
            owner: Arc::clone(&self.owner),
            binding: admitted.operation.binding(),
            plan: Mutex::new(Some(plan)),
        })))
    }
}

/// Opaque review retaining only original coordinates, credential fingerprint and core snapshot.
#[derive(uniffi::Object)]
pub struct SearchWithdrawalReview {
    owner: Arc<Owner>,
    binding: ChatBinding,
    plan: Mutex<Option<store::SearchWithdrawalRecovery>>,
}
#[uniffi::export]
impl SearchWithdrawalReview {
    /// Reacquire original existing credentials and queue only the shared core's search withdrawal.
    /// Repeated confirmation remains idempotent. This entry point performs no network request.
    ///
    /// # Errors
    /// Returns closed/changed review, original-slot admission or shared core/storage failures.
    pub fn confirm(&self) -> Result<SearchRecoverySaved, SearchRecoveryError> {
        let plan = self.plan.lock().map_err(poisoned)?;
        let plan = plan.as_ref().ok_or(SearchRecoveryError::Closed)?;
        let admitted = self.owner.open()?;
        if admitted.operation.binding() != self.binding {
            return Err(SearchRecoveryError::Changed);
        }
        admitted
            .store
            .confirm_search_withdrawal_recovery(admitted.operation.identity(), plan)?;
        Ok(admitted.store.search_visibility()?.into())
    }
    /// Retry only this review's explicitly confirmed record on a worker thread.
    /// No confirmation or signing occurs; backoff and unsupported servers keep it queued.
    ///
    /// # Errors
    /// Returns closed/changed review, original-slot admission, network or storage failures.
    pub fn retry(&self) -> Result<SearchRecoverySaved, SearchRecoveryError> {
        crate::profile_network::on_network_stack(|| Ok(self.retry_held()))?
    }

    /// Dismiss the shared review without signing, opening credentials or changing a profile.
    /// Generated object disposal releases one wrapper; dismissal invalidates every review reference.
    ///
    /// # Errors
    /// Returns unavailable review synchronization state.
    pub fn dismiss(&self) -> Result<(), SearchRecoveryError> {
        drop(self.plan.lock().map_err(poisoned)?.take());
        Ok(())
    }
}
impl SearchWithdrawalReview {
    fn retry_held(&self) -> Result<SearchRecoverySaved, SearchRecoveryError> {
        let plan = self.plan.lock().map_err(poisoned)?;
        let plan = plan.as_ref().ok_or(SearchRecoveryError::Closed)?;
        let admitted = self.owner.open()?;
        if admitted.operation.binding() != self.binding {
            return Err(SearchRecoveryError::Changed);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(network_failed)?
            .as_secs();
        if admitted
            .store
            .due_search_withdrawal_recovery(plan, now)?
            .is_some()
        {
            let network = crate::profile_network::active_network(&admitted.operation.slot().dir)?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(network_failed)?;
            runtime.block_on(send_recovery(&admitted.store, plan, &network, now))?;
        }
        Ok(admitted.store.search_visibility()?.into())
    }
}

async fn send_recovery(
    store: &Store,
    plan: &store::SearchWithdrawalRecovery,
    network: &dyapp_identity::network::Network,
    now: u64,
) -> Result<(), SearchRecoveryError> {
    let mut entry = crate::profile_network::connect(network).await?;
    let protocols = Arc::new(dyapp_p2p_net::Protocols::new(&network.name).map_err(network_failed)?);
    let mut reached = dyapp_client::search::connect_search_nodes(&mut entry, protocols, 2).await;
    dyapp_client::search::publish_recovery(store, plan, &mut reached.nodes, now).await?;
    Ok(())
}

fn network_failed(error: impl std::fmt::Display) -> SearchRecoveryError {
    SearchRecoveryError::Failed {
        reason: error.to_string(),
    }
}

#[cfg(test)]
mod tests;
