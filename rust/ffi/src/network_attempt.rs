//! A single network invocation whose cancellation owner exists before worker dispatch.

use crate::network::{CheckError, ConnectionCheck, NetworkSettings, SeedRow};
use dyapp_client::cancellation::Cancellation;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A single-use check attempt. Retries create a fresh object; idle attempts hold no identity lease.
#[derive(uniffi::Object)]
pub struct ConnectionAttempt {
    checker: Arc<ConnectionCheck>,
    cancellation: Cancellation,
    started: AtomicBool,
}

#[uniffi::export]
impl ConnectionAttempt {
    /// Prepare ownership without opening a key, waiting for admission or starting network work.
    #[uniffi::constructor]
    pub fn new(checker: Arc<ConnectionCheck>) -> Arc<Self> {
        Arc::new(Self {
            checker,
            cancellation: Cancellation::new(),
            started: AtomicBool::new(false),
        })
    }

    /// Cancel this attempt before dispatch, during admission or during actual network work.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Whether this attempt has been cancelled, including after its worker returned.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Run once off the UI thread, retaining admission until actual completion.
    ///
    /// # Errors
    /// Returns cancellation, repeated invocation, admission, storage or network failures.
    pub fn run(&self, draft: NetworkSettings, wait_ms: u32) -> Result<Vec<SeedRow>, CheckError> {
        if self.started.swap(true, Ordering::AcqRel) {
            return Err(CheckError::Failed {
                reason: "network attempt already started".into(),
            });
        }
        self.checker.run_attempt(draft, wait_ms, &self.cancellation)
    }
}
