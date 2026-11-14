//! Visit each ready owner independently; selection never chooses the background queue owner.
use crate::{
    CoreError, MobileIdentityRegistry, MobileSearchOutbox, MobileSlotResolution, SearchOutboxStatus,
};
use dyapp_client::store::{SearchOutboxSchedule, SearchOutboxWake as Wake};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// OS scheduling projection of the shared core's multi-owner wake decision.
#[derive(Debug, PartialEq, Eq, uniffi::Enum)]
pub enum SearchOutboxWake {
    /// No automatic search delivery wake is needed.
    Idle,
    /// Retry a failed or still-due queue using OS backoff.
    Retry,
    /// Earliest future deadline after all due queues have been attempted.
    At {
        /// Unix seconds.
        ready_at: u64,
        /// Relative milliseconds computed by core immediately before returning to the OS adapter.
        delay_ms: u64,
    },
}
impl TryFrom<Wake> for SearchOutboxWake {
    type Error = CoreError;
    fn try_from(wake: Wake) -> Result<Self, CoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CoreError::Failed(error.to_string()))?
            .as_millis();
        let now = u64::try_from(now).map_err(|error| CoreError::Failed(error.to_string()))?;
        Ok(match wake {
            Wake::Idle => Self::Idle,
            Wake::Retry => Self::Retry,
            Wake::At { unix_seconds } => Self::At {
                ready_at: unix_seconds,
                delay_ms: wake.delay_millis(now).unwrap_or(0),
            },
        })
    }
}

/// Keyless background inventory handle; ready owners reacquire admission one at a time.
#[derive(uniffi::Object)]
pub struct MobileSearchOutboxes {
    registry: Arc<MobileIdentityRegistry>,
}
#[uniffi::export]
impl MobileSearchOutboxes {
    /// Retain registry coordinates and the credential adapter, never an opened identity.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(registry: Arc<MobileIdentityRegistry>) -> Arc<Self> {
        Arc::new(Self { registry })
    }

    /// Attempt each inventoried ready owner's signed outbox without changing active selection.
    /// One failed owner cannot prevent the remaining queues from being visited.
    ///
    /// # Errors
    /// Returns inventory/clock errors; individual owner failures request platform backoff.
    pub fn poll(&self) -> Result<SearchOutboxWake, CoreError> {
        let mut wake = Wake::Idle;
        for slot in self.registry.list()? {
            let next = self.poll_owner(slot.id);
            wake = wake.merge(next.unwrap_or(Wake::Retry));
        }
        wake.try_into()
    }
}
impl MobileSearchOutboxes {
    fn poll_owner(&self, id: String) -> Result<Wake, CoreError> {
        let peer = match self.registry.inspect(id.clone())? {
            MobileSlotResolution::Ready { key, .. } => key.peer_id(),
            _ => return Ok(Wake::Idle),
        };
        let outbox = MobileSearchOutbox::new(Arc::clone(&self.registry), id, peer);
        let schedule = match outbox.poll()? {
            SearchOutboxStatus::Idle => SearchOutboxSchedule::Idle,
            SearchOutboxStatus::ReviewRequired => SearchOutboxSchedule::ReviewRequired,
            SearchOutboxStatus::Scheduled { ready_at } => SearchOutboxSchedule::At {
                unix_seconds: ready_at,
            },
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CoreError::Failed(error.to_string()))?
            .as_secs();
        Ok(Wake::after_attempt(schedule, now))
    }
}

#[cfg(test)]
mod tests;
