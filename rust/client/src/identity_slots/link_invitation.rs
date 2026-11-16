//! Source-bound receiving side shared by native clients.

use super::{KeyReader, ReceivedLink, Registry};
use crate::link::{Code, Invite, TTL_SECONDS};
use crate::Node;
use dyapp_identity::network::{self, Network, Settings};
use dyapp_identity::Identity;
use std::future::Future;

/// A waiting invitation owns secret keys, but holds no idle registry operation.
/// Frontends separately admit cancellation and their profile-view generation before installation.
pub struct LinkInvitation {
    registry: Registry,
    from: String,
    expected_peer: String,
    wizard: bool,
    waiting: Option<Waiting>,
}

struct Waiting {
    invite: Invite,
    device: Identity,
}

impl Registry {
    /// Create an invitation only for the selected, ready source owner.
    ///
    /// # Errors
    /// Returns source/credential admission failures, invalid expiry or key-generation errors.
    pub fn begin_link(
        &self,
        from: &str,
        expected_peer: &str,
        wizard: bool,
        keys: &dyn KeyReader,
        now: u64,
    ) -> Result<LinkInvitation, String> {
        let _guard = self.shared_lock()?;
        self.link_source(from, expected_peer, keys)?;
        now.checked_add(TTL_SECONDS).ok_or("Invalid link expiry")?;
        let device = Identity::generate();
        let mailbox = crate::mailbox(&device.public_key());
        let invite = Invite::new(mailbox, now).map_err(|error| error.to_string())?;
        Ok(LinkInvitation {
            registry: Self::new(self.root.clone(), self.layout),
            from: from.into(),
            expected_peer: expected_peer.into(),
            wizard,
            waiting: Some(Waiting { invite, device }),
        })
    }
}

impl LinkInvitation {
    /// Public QR payload; absent after a complete record has been transferred.
    #[must_use]
    pub fn code(&self) -> Option<Code> {
        self.waiting.as_ref().map(|waiting| *waiting.invite.code())
    }

    /// Poll using the source's actual active network and the existing platform dial adapter.
    /// The worker must own this future through actual completion, including caller cancellation.
    /// No view/cancellation mutex may be held across this network operation.
    ///
    /// # Errors
    /// Returns expired/spent invitations, changed source, network or wire-record failures.
    /// Transient errors retain the invitation; no slot is installed by this method.
    pub async fn poll_with<F>(
        &mut self,
        keys: &dyn KeyReader,
        now: u64,
        connect: impl FnOnce(Network) -> F,
    ) -> Result<Option<ReceivedLink>, String>
    where
        F: Future<Output = Result<Node, String>>,
    {
        let waiting = self
            .waiting
            .as_mut()
            .ok_or("Link record already received")?;
        // Check expiry before admission or dialing; this also erases an expired one-use key.
        waiting
            .invite
            .open(&[], now)
            .map_err(|error| error.to_string())?;
        let linked = {
            let operation = self
                .registry
                .operation(&self.from, &self.expected_peer, keys)?;
            self.registry
                .link_source(&self.from, &self.expected_peer, keys)?;
            let network = network::load(&operation.slot().dir, Settings::Active)
                .map_err(|error| error.to_string())?
                .ok_or("notConnected")?;
            let mut node = connect(network).await?;
            waiting
                .invite
                .receive(&mut node, &waiting.device, now)
                .await
                .map_err(|error| error.to_string())?
        };
        let Some(linked) = linked else {
            return Ok(None);
        };
        // Both node and operation are gone before the caller can enter an exclusive writer.
        let waiting = self.waiting.take().ok_or("Link record already received")?;
        Ok(Some(ReceivedLink::new(
            &self.registry,
            self.from.clone(),
            self.expected_peer.clone(),
            self.wizard,
            linked,
            waiting.device,
        )))
    }
}
