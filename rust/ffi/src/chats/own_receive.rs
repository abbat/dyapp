//! Native ownership only; directory admission and group maintenance stay in the shared core.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::{contact_verification::Generation, profiles::Profiles, session::Session, Node};

impl ChatSession {
    pub(super) async fn receive_owner_profiles(
        &self,
        session: &Session,
        node: &mut Node,
        generation: &Generation,
        now: u64,
    ) -> Result<Option<Profiles>, CoreError> {
        let Ok(remote) = node.get_profile(&self.key.identity.peer_id()).await else {
            // Preserve mailbox fallback, but never grant new membership from an unavailable read.
            return Ok(None);
        };
        let mut context = self.contact_context.lock().map_err(failed)?;
        context
            .accept_own(generation, session, remote, now)
            .map_err(failed)?;
        Ok(Some(context.profiles().clone()))
    }

    pub(super) async fn receive_contact_profiles(
        &self,
        session: &Session,
        node: &mut Node,
        generation: &Generation,
        now: u64,
    ) -> Result<Profiles, CoreError> {
        if let Ok(Some(peer)) =
            dyapp_client::contact_verification::refresh_target(session.store(), now)
        {
            if let Ok(remote) = node.get_profile(&peer).await {
                let mut context = self.contact_context.lock().map_err(failed)?;
                // Read failures/older authority remain best-effort; obsolete generations cannot commit.
                let _ =
                    context.accept_contact_if_current(generation, session.store(), &peer, remote);
            }
        }
        let context = self.contact_context.lock().map_err(failed)?;
        if !context.is_current(generation) {
            return Err(failed("notConnected"));
        }
        Ok(context.profiles().clone())
    }

    pub(super) async fn maintain_received_own(
        &self,
        session: &mut Session,
        nodes: &mut [Node],
        profiles: Option<&Profiles>,
        now: u64,
    ) {
        let Some(profiles) = profiles else {
            return;
        };
        let Some(own) = profiles.get(&self.key.identity.peer_id()) else {
            return;
        };
        let _ = session
            .maintain_own_devices(nodes, own, profiles.all(), now, &self.seal_key)
            .await;
    }
}
