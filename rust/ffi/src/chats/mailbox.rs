//! Native mailbox I/O retains profile authority in the generation-checked common context.
use super::{failed, ChatSession};
use crate::CoreError;
use dyapp_client::{chat::Event, contact_verification::Generation, session::Session, Node};
use std::collections::BTreeSet;

impl ChatSession {
    pub(super) async fn apply_mailbox(
        &self,
        session: &mut Session,
        node: &mut Node,
        nodes: &mut [Node],
        generation: &Generation,
        now: u64,
    ) -> Result<u32, CoreError> {
        let mut profiles = self.receive_profiles(generation)?;
        let mut attempted = BTreeSet::new();
        let mut changed = 0u32;
        for _ in 0..2 {
            let events = session
                .receive(
                    nodes,
                    &self.device_key.identity,
                    profiles.all(),
                    &self.seal_key,
                )
                .await
                .map_err(failed)?;
            let (events_changed, fetched) = self
                .receive_events(session, node, events, generation, now, &mut attempted)
                .await?;
            changed = changed.saturating_add(events_changed);
            if !fetched {
                break;
            }
            profiles = self.receive_profiles(generation)?;
        }
        Ok(changed)
    }

    async fn receive_events(
        &self,
        session: &Session,
        node: &mut Node,
        events: Vec<Event>,
        generation: &Generation,
        now: u64,
        attempted: &mut BTreeSet<String>,
    ) -> Result<(u32, bool), CoreError> {
        let mut changed = 0u32;
        let mut fetched = false;
        for event in events {
            match event {
                Event::UnknownOwner(peer) => {
                    // An older held profile must not prevent a newly listed device from refreshing.
                    if attempted.insert(peer.clone()) {
                        fetched |= self
                            .receive_profile(session, node, generation, &peer, now)
                            .await?;
                    }
                }
                Event::Gap(_) | Event::Refused(_) => {}
                _ => changed = changed.saturating_add(1),
            }
        }
        Ok((changed, fetched))
    }

    async fn receive_profile(
        &self,
        session: &Session,
        node: &mut Node,
        generation: &Generation,
        peer: &str,
        now: u64,
    ) -> Result<bool, CoreError> {
        let Ok(remote) = node.get_profile(peer).await else {
            return Ok(false);
        };
        let mut context = self.contact_context.lock().map_err(failed)?;
        if peer == session.store().owner_peer_id() {
            context
                .accept_own(generation, session, remote, now)
                .map_err(failed)?;
        } else {
            let admitted = context
                .accept_contact_if_current(generation, session.store(), peer, remote)
                .unwrap_or(false);
            if !admitted && context.is_current(generation) {
                return Ok(false);
            }
        }
        if !context.is_current(generation) {
            return Err(failed("notConnected"));
        }
        Ok(true)
    }
}
