//! Interaction commitment is separate from delivery and from a no-event result.

use super::{Body, Error, Interaction, Kind, Node, Session, VerifiedProfile};

/// An interaction that was committed with its prepared writes before delivery was attempted.
#[derive(Debug)]
pub struct InteractionSubmission {
    /// The actual persisted protocol event, including its stable ID and HLC.
    pub event: Interaction,
    /// Node acceptance or the immediate delivery failure; never a peer receipt.
    pub delivery: Result<(), Error>,
}

impl InteractionSubmission {
    /// Require node acceptance for callers retaining the strict interaction contract.
    ///
    /// # Errors
    /// Returns the committed event's immediate delivery failure.
    pub fn into_accepted(self) -> Result<Interaction, Error> {
        self.delivery?;
        Ok(self.event)
    }
}

impl Session {
    /// Submit an interaction to the canonical pair group, inviting the peer for its first like.
    /// A successful `None` means no event was created, never a queued event.
    ///
    /// # Errors
    /// Returns peer selection, blocked-peer, key-package or staging errors before commitment
    /// of the interaction. A separately staged invitation may already exist.
    pub async fn submit_interaction_with(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        kind: Kind,
        seal_key: &[u8; 32],
    ) -> Result<Option<InteractionSubmission>, Error> {
        let peer = self.peer_in(profiles)?;
        if self.store.blocked(&peer)? {
            return Err(crate::store::Error::BlockedPeer.into());
        }
        if let Some(group_id) = self.pair_group(&peer) {
            return self
                .submit_interaction(nodes, &group_id, kind, seal_key)
                .await;
        }
        if kind != Kind::Like {
            return Ok(None);
        }
        let packages = self.key_packages(nodes, profiles).await?;
        let Some((event, writes)) = self.stage_first_like(profiles, &packages, seal_key)? else {
            return Ok(None);
        };
        let delivery = self.send_staged(nodes, writes, seal_key).await;
        Ok(Some(InteractionSubmission { event, delivery }))
    }

    /// Commit an interaction in a joined group, retaining its event on delivery failure.
    /// A disabled/already-sent daily view returns `None` without staging.
    ///
    /// # Errors
    /// Returns unknown-group, draft or staging errors before interaction commitment.
    pub async fn submit_interaction(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        kind: Kind,
        seal_key: &[u8; 32],
    ) -> Result<Option<InteractionSubmission>, Error> {
        let target = self.partner(group_id).ok_or(Error::UnknownGroup)?;
        let wall_ms = crate::now() * 1000;
        let Some(event) = self.store.draft_interaction(&target, kind, wall_ms)? else {
            return Ok(None);
        };
        let writes = self.stage_message(group_id, &Body::Interaction(event), seal_key)?;
        let delivery = self.send_staged(nodes, writes, seal_key).await;
        Ok(Some(InteractionSubmission { event, delivery }))
    }
}
