//! The search index rebuild worker and the catch-up round that ends a refill.

use crate::service::Service;
use std::sync::Arc;

/// Rebuilds the search index into a new generation off the loop; searches go on meanwhile.
pub fn spawn_rebuild(service: &Arc<Service>) {
    let Some(index) = service.search.clone() else {
        return;
    };
    let service = service.clone();
    tokio::spawn(async move {
        let min_free = service.config.limits.min_free_mb.saturating_mul(1 << 20);
        index.rebuild(&service.rebuild_budget, min_free).await;
    });
}

/// A catch-up round run while the index refills: `Some(answered)` while it runs.
#[derive(Default)]
pub struct Round(Option<bool>);

impl Round {
    /// Starts a round at a catch-up tick if the index refills and none runs.
    pub fn start(&mut self, service: &Service) {
        let refilling = service
            .search
            .as_ref()
            .is_some_and(|index| index.refilling());
        if self.0.is_none() && refilling {
            self.0 = Some(false);
        }
    }

    /// A peer answered an inventory request.
    pub fn answered(&mut self) {
        if let Some(answered) = self.0.as_mut() {
            *answered = true;
        }
    }

    /// Ends the round once nothing is pending; a round in which a peer answered ends refilling.
    pub fn drained(&mut self, service: &Service) {
        if self.0.take() != Some(true) {
            return;
        }
        if let Some(Err(error)) = service.search.as_ref().map(|index| index.refilled()) {
            tracing::error!(%error, "search index refilling not ended");
        }
    }
}
