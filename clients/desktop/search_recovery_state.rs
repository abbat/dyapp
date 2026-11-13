//! Opaque desktop review handles; authorization and delivery policy live in the shared core.
use dyapp_client::store::{SearchVisibility, SearchWithdrawalRecovery, Store};
use dyapp_identity::Identity;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    ) -> Result<Option<String>, String> {
        let mut reviews = self.0.lock().map_err(|error| error.to_string())?;
        let store = Store::open_existing(&dir.join("client.db"), identity)
            .map_err(|error| error.to_string())?;
        let Some(plan) = store
            .prepare_search_withdrawal_recovery()
            .map_err(|error| error.to_string())?
        else {
            reviews.current = None;
            return Ok(None);
        };
        reviews.next = reviews
            .next
            .checked_add(1)
            .ok_or("Search recovery handle exhausted")?;
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
    ) -> Result<Saved, String> {
        self.with_review(identity, dir, generation, token, |store, plan| {
            store
                .confirm_search_withdrawal_recovery(identity, plan)
                .map_err(|error| error.to_string())?;
            store
                .search_visibility()
                .map(Saved::from)
                .map_err(|error| error.to_string())
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
    ) -> Result<Saved, String> {
        self.with_review(identity, dir, generation, token, |store, plan| {
            if store
                .due_search_withdrawal_recovery(plan, now)
                .map_err(|error| error.to_string())?
                .is_some()
            {
                send(store, plan)?;
            }
            store
                .search_visibility()
                .map(Saved::from)
                .map_err(|error| error.to_string())
        })
    }

    fn with_review<T>(
        &self,
        identity: &Identity,
        dir: &Path,
        generation: u64,
        token: &str,
        work: impl FnOnce(&Store, &SearchWithdrawalRecovery) -> Result<T, String>,
    ) -> Result<T, String> {
        let reviews = self.0.lock().map_err(|error| error.to_string())?;
        let review = reviews
            .current
            .as_ref()
            .filter(|review| review.token == token)
            .ok_or("Search recovery review is closed")?;
        if review.peer != identity.peer_id() || review.dir != dir || review.generation != generation
        {
            return Err("Search recovery review changed".into());
        }
        let store = Store::open_existing(&dir.join("client.db"), identity)
            .map_err(|error| error.to_string())?;
        work(&store, &review.plan)
    }

    pub(super) fn dismiss(&self, token: &str) -> Result<(), String> {
        let mut reviews = self.0.lock().map_err(|error| error.to_string())?;
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
