//! Owner approval binds one immutable target and selected source without an idle lease.

use super::{KeyReader, ProfileGeneration, ProfileView, Registry};
use crate::link::{parse_input, Code};
use crate::store::Store;
use crate::Node;
use dyapp_identity::network::{self, Network, Settings};
use std::future::Future;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Prepared target only. Preparation never sends identity material or private state.
pub struct LinkSender {
    registry: Registry,
    from: String,
    expected_peer: String,
    generation: ProfileGeneration,
    code: Code,
}

impl Registry {
    /// Prepare the exact target shown in an owner's full-control confirmation.
    ///
    /// # Errors
    /// Returns malformed/expired input, stale source, credential or poisoned-view failures.
    pub fn prepare_link_sender(
        &self,
        view: &ProfileView,
        from: &str,
        expected_peer: &str,
        text: &str,
        keys: &dyn KeyReader,
        now: u64,
    ) -> Result<LinkSender, String> {
        let code = parse_input(text, now).map_err(|error| error.to_string())?;
        let view = view.current()?;
        let _guard = self.shared_lock()?;
        self.link_source(from, expected_peer, keys)?;
        Ok(LinkSender {
            registry: Self::new(self.root.clone(), self.layout),
            from: from.into(),
            expected_peer: expected_peer.into(),
            generation: view.generation(),
            code,
        })
    }
}

impl LinkSender {
    /// Immutable public target that was prepared for confirmation.
    #[must_use]
    pub fn code(&self) -> Code {
        self.code
    }

    /// Send only after explicit owner confirmation of this exact prepared target.
    /// The actual worker must retain this future and its operation through completion.
    /// The snapshot never crosses a foreign boundary and is erased on every exit.
    ///
    /// # Errors
    /// Refuses changed view/source/owner and expired codes before dialing; returns snapshot,
    /// settings and delivery failures. Some encrypted parts may already be sent on network error.
    pub async fn confirm_and_send_with<F>(
        &self,
        keys: &dyn KeyReader,
        view: &ProfileView,
        now: u64,
        connect: impl FnOnce(Network, PathBuf) -> F,
    ) -> Result<(), String>
    where
        F: Future<Output = Result<Node, String>>,
    {
        let view = view.admit(&self.generation)?;
        let code = parse_input(&self.code.text(), now).map_err(|error| error.to_string())?;
        let operation = self
            .registry
            .operation(&self.from, &self.expected_peer, keys)?;
        self.registry
            .link_source(&self.from, &self.expected_peer, keys)?;
        let state = {
            let store = Store::open(
                &operation.slot().dir.join("client.db"),
                operation.identity(),
            )
            .map_err(|error| error.to_string())?;
            Zeroizing::new(store.link_state().map_err(|error| error.to_string())?)
        };
        let network = network::load(&operation.slot().dir, Settings::Active)
            .map_err(|error| error.to_string())?
            .ok_or("notConnected")?;
        drop(view);
        let mut node = connect(network, operation.slot().dir.clone()).await?;
        code.send(&mut node, operation.identity(), &state, now)
            .await
            .map_err(|error| error.to_string())
    }
}
