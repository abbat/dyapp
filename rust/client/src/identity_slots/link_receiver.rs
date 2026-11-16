//! Persistent receiver ownership with adapter-specific, once-only installation receipts.
#[cfg(test)]
mod tests;

use super::{
    KeyReader, LinkAdmission, LinkInvitation, LinkPhase, LinkWrite, ProfileGeneration, ProfileView,
    ReceivedLink,
};
use crate::link::Code;
use crate::Node;
use dyapp_identity::network::Network;
use std::future::Future;
use std::sync::{Mutex, MutexGuard};

struct State<T, E> {
    invitation: Option<LinkInvitation>,
    received: Option<ReceivedLink>,
    result: Option<Result<T, E>>,
}

/// Shared receiver lifecycle; adapters supply only connection, credential IO and receipt mapping.
/// Retain the actual worker through completion; idle receivers hold no registry/view lease.
pub struct LinkReceiver<T, E> {
    admission: LinkAdmission,
    generation: ProfileGeneration,
    code: Code,
    state: Mutex<State<T, E>>,
}

impl<T, E> LinkReceiver<T, E> {
    /// Take ownership after capturing invitation and generation under the same view guard.
    ///
    /// # Errors
    /// Refuses an invitation whose record was already transferred.
    pub fn new(invitation: LinkInvitation, generation: ProfileGeneration) -> Result<Self, String> {
        let code = invitation.code().ok_or("Link record already received")?;
        Ok(Self {
            admission: LinkAdmission::default(),
            generation,
            code,
            state: Mutex::new(State {
                invitation: Some(invitation),
                received: None,
                result: None,
            }),
        })
    }

    /// Original public QR payload. Display it only while the receiver is waiting/receiving.
    #[must_use]
    pub fn code(&self) -> Code {
        self.code
    }

    /// Non-blocking lifecycle observation, including while a platform writer runs.
    #[must_use]
    pub fn phase(&self) -> LinkPhase {
        self.admission.phase()
    }

    /// Cancel before local write admission; otherwise report the actual uncancellable phase.
    #[must_use]
    pub fn cancel(&self) -> LinkPhase {
        self.admission.cancel()
    }

    /// Copy the first adapter receipt without reopening credentials or re-running installation.
    ///
    /// # Errors
    /// A poisoned state remains explicit recovery, never an empty receipt or retry permission.
    pub fn result(&self) -> Result<Option<Result<T, E>>, String>
    where
        T: Clone,
        E: Clone,
    {
        Ok(self.state()?.result.clone())
    }

    /// Receive once and admit local installation through the original view generation.
    /// No state/view mutex is held during network work. The received record is retained in this
    /// object before invoking the adapter, including when its credential callback unwinds.
    ///
    /// # Errors
    /// Returns network, expired code, stale view or poisoned-state failures.
    /// Normal transport failures retain the invitation; installation results are cached separately.
    pub async fn poll_with<F>(
        &self,
        keys: &dyn KeyReader,
        view: &ProfileView,
        now: u64,
        connect: impl FnOnce(Network) -> F,
        install: impl FnOnce(&mut ReceivedLink) -> Result<T, E>,
    ) -> Result<LinkPhase, String>
    where
        F: Future<Output = Result<Node, String>>,
    {
        let poll = match self.admission.receive() {
            Ok(poll) => poll,
            Err(phase) => return Ok(phase),
        };
        let mut invitation = self
            .state()?
            .invitation
            .take()
            .ok_or("Link invitation unavailable")?;
        match invitation.poll_with(keys, now, connect).await {
            Err(error) => {
                self.state()?.invitation = Some(invitation);
                let _ = poll.resume();
                Err(error)
            }
            Ok(None) => {
                self.state()?.invitation = Some(invitation);
                Ok(poll.resume())
            }
            Ok(Some(received)) => {
                self.state()?.received = Some(received);
                let write = poll.install(view, &self.generation)?;
                self.install(write, install)
            }
        }
    }

    fn install(
        &self,
        write: LinkWrite<'_>,
        install: impl FnOnce(&mut ReceivedLink) -> Result<T, E>,
    ) -> Result<LinkPhase, String> {
        let mut state = self.state()?;
        let received = state.received.as_mut().ok_or("Link record unavailable")?;
        let result = install(received);
        let succeeded = result.is_ok();
        state.result = Some(result);
        write.finish(succeeded);
        Ok(self.phase())
    }

    fn state(&self) -> Result<MutexGuard<'_, State<T, E>>, String> {
        self.state
            .lock()
            .map_err(|_| "Link receiver state poisoned".into())
    }
}
