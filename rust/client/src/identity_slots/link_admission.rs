//! Non-blocking receiver cancellation and local write admission.

use super::{ProfileGeneration, ProfileView, ProfileViewGuard};
use std::sync::atomic::{AtomicU8, Ordering};

/// Receiver progress, not a claim about destination lifecycle or rollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LinkPhase {
    /// Another poll may start.
    Waiting,
    /// One worker owns actual network work.
    Receiving,
    /// Cancellation won before local installation admission.
    Cancelled,
    /// Local installation was admitted; cancellation cannot roll it back.
    Installing,
    /// The writer returned success; its receipt remains authoritative.
    Completed,
    /// The writer returned an error; its receipt must be retained.
    Failed,
    /// Work left without a normal completion; never retry implicitly.
    Interrupted,
    /// The original frontend generation no longer admits installation.
    Invalidated,
}

impl LinkPhase {
    fn from_byte(value: u8) -> Self {
        match value {
            0 => Self::Waiting,
            1 => Self::Receiving,
            2 => Self::Cancelled,
            3 => Self::Installing,
            4 => Self::Completed,
            5 => Self::Failed,
            7 => Self::Invalidated,
            _ => Self::Interrupted,
        }
    }
}

/// One invitation's admission scope. Idle instances hold no identity or view lease.
#[derive(Debug, Default)]
pub struct LinkAdmission(AtomicU8);

impl LinkAdmission {
    /// Observe progress without waiting for network or credential callbacks.
    #[must_use]
    pub fn phase(&self) -> LinkPhase {
        LinkPhase::from_byte(self.0.load(Ordering::SeqCst))
    }

    /// Claim the only receiving worker.
    ///
    /// # Errors
    /// Returns the current non-waiting phase; no network operation may start.
    pub fn receive(&self) -> Result<LinkPoll<'_>, LinkPhase> {
        self.transition(LinkPhase::Waiting, LinkPhase::Receiving)?;
        Ok(LinkPoll {
            admission: Some(self),
        })
    }

    /// Cancel before installation, or report the phase that can no longer be cancelled.
    #[must_use]
    pub fn cancel(&self) -> LinkPhase {
        let changed = self
            .0
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                matches!(
                    LinkPhase::from_byte(value),
                    LinkPhase::Waiting | LinkPhase::Receiving
                )
                .then_some(LinkPhase::Cancelled as u8)
            });
        match changed {
            Ok(_) => LinkPhase::Cancelled,
            Err(value) => LinkPhase::from_byte(value),
        }
    }

    fn transition(&self, from: LinkPhase, to: LinkPhase) -> Result<LinkPhase, LinkPhase> {
        self.0
            .compare_exchange(from as u8, to as u8, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| to)
            .map_err(LinkPhase::from_byte)
    }
}

/// Retain through actual receive completion, including cancellation by a UI caller.
#[must_use = "an abandoned receive is interrupted, not automatically retried"]
pub struct LinkPoll<'a> {
    admission: Option<&'a LinkAdmission>,
}

impl<'a> LinkPoll<'a> {
    /// A normal empty/transient result permits another poll unless cancellation already won.
    #[must_use]
    pub fn resume(mut self) -> LinkPhase {
        // Disarm before reopening reception: a new poll may start immediately.
        self.admission
            .take()
            .map_or(LinkPhase::Interrupted, |admission| {
                admission
                    .transition(LinkPhase::Receiving, LinkPhase::Waiting)
                    .unwrap_or_else(|phase| phase)
            })
    }

    /// Admit the original frontend generation after releasing all shared identity operations.
    /// Keep the received record in its persistent owner before entering platform callbacks.
    ///
    /// # Errors
    /// Refuses cancellation, stale generations and poisoned views before any installation.
    pub fn install(
        mut self,
        view: &'a ProfileView,
        generation: &ProfileGeneration,
    ) -> Result<LinkWrite<'a>, String> {
        let admission = self.admission.ok_or("Link poll already consumed")?;
        let mut guard = match view.admit(generation) {
            Ok(guard) => guard,
            Err(error) => {
                let _ = admission.transition(LinkPhase::Receiving, LinkPhase::Invalidated);
                return Err(error);
            }
        };
        admission
            .transition(LinkPhase::Receiving, LinkPhase::Installing)
            .map_err(|_| "Link installation was cancelled".to_owned())?;
        guard.invalidate();
        let _ = self.admission.take();
        Ok(LinkWrite {
            admission,
            _view: guard,
        })
    }
}

impl Drop for LinkPoll<'_> {
    fn drop(&mut self) {
        if let Some(admission) = self.admission {
            let _ = admission.transition(LinkPhase::Receiving, LinkPhase::Interrupted);
        }
    }
}

/// Local installation guard; never hold it during network work.
#[must_use = "a writer without a normal receipt remains interrupted"]
pub struct LinkWrite<'a> {
    admission: &'a LinkAdmission,
    _view: ProfileViewGuard<'a>,
}

impl LinkWrite<'_> {
    /// Call only after the persistent received record has retained the actual writer receipt.
    pub fn finish(self, succeeded: bool) {
        let phase = if succeeded {
            LinkPhase::Completed
        } else {
            LinkPhase::Failed
        };
        let _ = self.admission.transition(LinkPhase::Installing, phase);
    }
}

impl Drop for LinkWrite<'_> {
    fn drop(&mut self) {
        let _ = self
            .admission
            .transition(LinkPhase::Installing, LinkPhase::Interrupted);
    }
}

#[cfg(test)]
mod tests;
