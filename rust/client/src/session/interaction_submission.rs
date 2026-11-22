//! Interaction commitment is separate from delivery and from a no-event result.

use super::{prepare, Body, Error, Interaction, Kind, Node, Session, VerifiedProfile};
use crate::{chat, PreparedEnvelope};
use dyapp_p2p_net::proto::EnvelopeClass;

/// An interaction that was committed with its prepared writes before delivery was attempted.
#[derive(Debug)]
pub struct InteractionSubmission {
    /// The actual persisted protocol event, including its stable ID and HLC.
    pub event: Interaction,
    /// Node acceptance or the immediate delivery failure; never a peer receipt.
    pub delivery: Result<(), Error>,
}

impl From<InteractionSubmission> for super::Submission {
    fn from(value: InteractionSubmission) -> Self {
        let id = value.event.event_id;
        match value.delivery {
            Ok(()) => Self::Accepted { id },
            Err(error) => Self::Queued { id, error },
        }
    }
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
    /// Submit to the canonical pair, inviting for a first like or enabled profile view.
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
        if !self.initial_interaction_pending(&peer, kind)? {
            return Ok(None);
        }
        let packages = self.key_packages(nodes, profiles).await?;
        let staged = match kind {
            Kind::Like => self.stage_first_like(profiles, &packages, seal_key)?,
            Kind::View => self.stage_first_view(profiles, &packages, crate::now_ms(), seal_key)?,
            Kind::Unlike => return Ok(None),
        };
        let Some((event, writes)) = staged else {
            return Ok(None);
        };
        let delivery = self.send_staged(nodes, writes, seal_key).await;
        Ok(Some(InteractionSubmission { event, delivery }))
    }

    fn initial_interaction_pending(&self, peer: &str, kind: Kind) -> Result<bool, Error> {
        match kind {
            Kind::Like => Ok(true),
            Kind::Unlike => Ok(false),
            // Share drafting policy without allocating a throwaway random event ID.
            Kind::View => Ok(self.store.profile_view_due_at(peer, crate::now_ms())?),
        }
    }

    /// Stage a first View at an explicit clock value; the caller has no joined pair yet.
    pub(super) fn stage_first_view(
        &mut self,
        profiles: &[VerifiedProfile],
        packages: &[(Vec<u8>, Vec<u8>)],
        wall_ms: u64,
        seal_key: &[u8; 32],
    ) -> Result<Option<(Interaction, Vec<PreparedEnvelope>)>, Error> {
        let peer = self.peer_in(profiles)?;
        let Some(event) = self.store.draft_interaction(&peer, Kind::View, wall_ms)? else {
            return Ok(None);
        };
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let staged = (|| {
            let (mut group, welcome) = chat::create(device, packages, profiles)?;
            let mut writes = prepare(&self.store, welcome, EnvelopeClass::Message, seal_key)?;
            let deliveries = chat::message(&mut group, device, &Body::Interaction(event))?;
            writes.extend(prepare(
                &self.store,
                deliveries,
                EnvelopeClass::Signal,
                seal_key,
            )?);
            self.store.stage_invite_interaction(
                &group,
                &peer,
                &device.device_key()?,
                event,
                &device.seal(seal_key)?,
                &writes,
            )?;
            Ok((group, writes))
        })();
        match staged {
            Ok((group, writes)) => {
                self.groups.push(group);
                Ok(Some((event, writes)))
            }
            Err(error) => {
                self.device = None;
                self.groups.clear();
                Err(error)
            }
        }
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
        let wall_ms = crate::now_ms();
        let Some(event) = self.store.draft_interaction(&target, kind, wall_ms)? else {
            return Ok(None);
        };
        let writes = self.stage_message(group_id, &Body::Interaction(event), seal_key)?;
        let delivery = self.send_staged(nodes, writes, seal_key).await;
        Ok(Some(InteractionSubmission { event, delivery }))
    }
}
