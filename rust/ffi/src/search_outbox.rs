//! Reopen and deliver an original slot's already signed search outbox after process restart.
use crate::profile_network::{active_network, connect, on_network_stack};
use crate::slot_operation::SlotWorker;
use crate::{CoreError, MobileIdentityRegistry};
use dyapp_client::store::{SearchOutboxSchedule, Store};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Scheduling information only; no proof of removal from every search server.
#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SearchOutboxStatus {
    /// No signed record remains queued.
    Idle,
    /// A legacy withdrawal needs explicit owner confirmation; polling never signs it.
    ReviewRequired,
    /// The exact earliest permitted send time; a past time is due now.
    Scheduled {
        /// Unix seconds, including ordinary update windows and explicit server backoff.
        ready_at: u64,
    },
}
impl From<SearchOutboxSchedule> for SearchOutboxStatus {
    fn from(schedule: SearchOutboxSchedule) -> Self {
        match schedule {
            SearchOutboxSchedule::Idle => Self::Idle,
            SearchOutboxSchedule::ReviewRequired => Self::ReviewRequired,
            SearchOutboxSchedule::At { unix_seconds } => Self::Scheduled {
                ready_at: unix_seconds,
            },
        }
    }
}

/// Idle original-slot coordinates only; every call reacquires existing identity admission.
#[derive(uniffi::Object)]
pub struct MobileSearchOutbox {
    slot: SlotWorker,
}
#[uniffi::export]
impl MobileSearchOutbox {
    /// Bind an existing owner without opening keys, chats or the database while idle.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        registry: Arc<MobileIdentityRegistry>,
        id: String,
        expected_peer: String,
    ) -> Arc<Self> {
        Arc::new(Self {
            slot: SlotWorker::new(registry, id, expected_peer),
        })
    }

    /// Read the persisted queue schedule, including after a restart. An absent database is idle.
    ///
    /// # Errors
    /// Returns original-slot admission, malformed state or storage errors.
    pub fn status(&self) -> Result<SearchOutboxStatus, CoreError> {
        self.with_store(|store, _| {
            store
                .search_outbox_schedule()
                .map(Into::into)
                .map_err(failed)
        })
    }

    /// Send only an already signed, authorized record when due; never confirms or signs intent.
    /// The original owner is retained through actual network completion, including UI cancellation.
    ///
    /// # Errors
    /// Returns original-slot admission, network, malformed state or storage errors.
    /// A network failure leaves the signed record queued for a later lifecycle wake.
    pub fn poll(&self) -> Result<SearchOutboxStatus, CoreError> {
        on_network_stack(|| {
            self.with_store(|store, dir| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(failed)?.as_secs();
            let schedule = store.search_outbox_schedule().map_err(failed)?;
            if !matches!(schedule, SearchOutboxSchedule::At { unix_seconds } if now >= unix_seconds) {
                return Ok(schedule.into());
            }
            let network = active_network(dir)?;
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(failed)?;
            runtime.block_on(send(store, &network, now))?;
            store.search_outbox_schedule().map(Into::into).map_err(failed)
        })
        })
    }
}
impl MobileSearchOutbox {
    fn with_store(
        &self,
        work: impl FnOnce(&Store, &Path) -> Result<SearchOutboxStatus, CoreError>,
    ) -> Result<SearchOutboxStatus, CoreError> {
        let admitted = self.slot.operation()?;
        let operation = &admitted.operation;
        let dir = &operation.slot().dir;
        let path = dir.join("client.db");
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SearchOutboxStatus::Idle)
            }
            Err(error) => return Err(failed(error)),
            Ok(_) => {}
        }
        let store = Store::open_existing(&path, operation.identity()).map_err(failed)?;
        work(&store, dir)
    }
}
async fn send(
    store: &Store,
    network: &dyapp_identity::network::Network,
    now: u64,
) -> Result<(), CoreError> {
    let mut entry = connect(network).await?;
    let protocols = Arc::new(dyapp_p2p_net::Protocols::new(&network.name).map_err(failed)?);
    let mut reached = dyapp_client::search::connect_search_nodes(&mut entry, protocols, 2).await;
    dyapp_client::search::publish_due(store, &mut reached.nodes, now)
        .await
        .map_err(failed)?;
    Ok(())
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

#[cfg(test)]
mod tests;
