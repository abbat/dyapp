//! Opaque desktop review handles; authorization and delivery policy live in the shared core.
use dyapp_client::store::{self, SearchVisibility, SearchWithdrawalRecovery, Store};
use dyapp_identity::Identity;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Typed transport refusals; presentation never classifies backend diagnostic strings.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum Failure {
    Closed,
    Changed,
    ReviewRequired,
    LinkPending,
    VersionExhausted,
    WrongOwner,
    Failed { reason: String },
}
impl Failure {
    pub(super) fn failed(error: impl std::fmt::Display) -> Self {
        Self::Failed {
            reason: error.to_string(),
        }
    }
}
impl From<store::Error> for Failure {
    fn from(error: store::Error) -> Self {
        match error {
            store::Error::SearchWithdrawalRecoveryChanged => Self::Changed,
            store::Error::SearchWithdrawalReviewRequired => Self::ReviewRequired,
            store::Error::LinkStateReviewRequired => Self::LinkPending,
            store::Error::ProfileVersionExhausted => Self::VersionExhausted,
            store::Error::WrongIdentity => Self::WrongOwner,
            error => Self::failed(error),
        }
    }
}

/// Current local state, not proof of network-wide removal.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Saved {
    version: u64,
    hidden: bool,
    search_pending: bool,
}
impl From<SearchVisibility> for Saved {
    fn from(state: SearchVisibility) -> Self {
        Self {
            version: state.version,
            hidden: state.hidden,
            search_pending: state.search_pending,
        }
    }
}
struct Review {
    token: String,
    peer: String,
    dir: PathBuf,
    generation: u64,
    plan: SearchWithdrawalRecovery,
}
#[derive(Default)]
struct Reviews {
    next: u64,
    current: Option<Review>,
}
/// One outstanding review; idle state retains no identity key or admission lease.
#[derive(Clone, Default)]
pub(super) struct RecoveryState(Arc<Mutex<Reviews>>);

impl RecoveryState {
    pub(super) fn prepare(
        &self,
        identity: &Identity,
        dir: &Path,
        generation: u64,
    ) -> Result<Option<String>, Failure> {
        let mut reviews = self.0.lock().map_err(Failure::failed)?;
        let store = Store::open_existing(&dir.join("client.db"), identity)?;
        let Some(plan) = store.prepare_search_withdrawal_recovery()? else {
            reviews.current = None;
            return Ok(None);
        };
        reviews.next = reviews
            .next
            .checked_add(1)
            .ok_or_else(|| Failure::failed("Search recovery handle exhausted"))?;
        let token = reviews.next.to_string();
        reviews.current = Some(Review {
            token: token.clone(),
            peer: identity.peer_id(),
            dir: dir.into(),
            generation,
            plan,
        });
        Ok(Some(token))
    }

    pub(super) fn confirm(
        &self,
        identity: &Identity,
        dir: &Path,
        generation: u64,
        token: &str,
    ) -> Result<Saved, Failure> {
        self.with_review(identity, dir, generation, token, |store, plan| {
            store.confirm_search_withdrawal_recovery(identity, plan)?;
            store
                .search_visibility()
                .map(Saved::from)
                .map_err(Failure::from)
        })
    }

    pub(super) fn retry(
        &self,
        identity: &Identity,
        dir: &Path,
        generation: u64,
        token: &str,
        now: u64,
        send: impl FnOnce(&Store, &SearchWithdrawalRecovery) -> Result<(), String>,
    ) -> Result<Saved, Failure> {
        self.with_review(identity, dir, generation, token, |store, plan| {
            if store.due_search_withdrawal_recovery(plan, now)?.is_some() {
                send(store, plan).map_err(Failure::failed)?;
            }
            store
                .search_visibility()
                .map(Saved::from)
                .map_err(Failure::from)
        })
    }

    fn with_review<T>(
        &self,
        identity: &Identity,
        dir: &Path,
        generation: u64,
        token: &str,
        work: impl FnOnce(&Store, &SearchWithdrawalRecovery) -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let reviews = self.0.lock().map_err(Failure::failed)?;
        let review = reviews
            .current
            .as_ref()
            .filter(|review| review.token == token)
            .ok_or(Failure::Closed)?;
        if review.peer != identity.peer_id() {
            return Err(Failure::WrongOwner);
        }
        if review.dir != dir || review.generation != generation {
            return Err(Failure::Changed);
        }
        let store = Store::open_existing(&dir.join("client.db"), identity)?;
        work(&store, &review.plan)
    }

    pub(super) fn dismiss(&self, token: &str) -> Result<(), Failure> {
        let mut reviews = self.0.lock().map_err(Failure::failed)?;
        if reviews
            .current
            .as_ref()
            .is_some_and(|review| review.token == token)
        {
            reviews.current = None;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "search_recovery_state_tests.rs"]
mod tests;
