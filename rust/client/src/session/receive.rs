//! The receive path: fetch, apply and commit each new envelope, then acknowledge and receipt.

use super::{Error, Session, StoredMessage};
use crate::chat::{self, Body, Event, Receipt, MAX_RECEIPT_IDS};
use crate::scores;
use crate::store::messages::VerifiedChatSender;
use crate::store::{Conversation, InboxKey, InboxKind};
use crate::Node;
use dyapp_identity::Identity;
use dyapp_profile::VerifiedProfile;

impl Session {
    /// Fetch the mailbox of `device_key` from the first of `nodes` by score that answers, commit each new envelope before returning it, then
    /// acknowledge every committed ID, earlier ones whose ACK was lost included, and send each
    /// group one receipt for its new envelopes. Write-token and receipt bodies are handled here
    /// and not returned; a join also asks the group for tokens. On an error, events committed so
    /// far are in the inbox history and their ACKs stay pending.
    pub async fn receive(
        &mut self,
        nodes: &mut [Node],
        device_key: &Identity,
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Vec<Event>, Error> {
        let (at, (envelopes, _more)) =
            scores::first(&mut self.scores, nodes, Node::peer, async |node| {
                Ok((node.peer(), node.fetch(device_key).await?))
            })
            .await?;
        let mailbox = crate::mailbox(&device_key.public_key());
        let mut events = Vec::new();
        let mut seen = Vec::new();
        let mut receipts: Vec<(Vec<u8>, Vec<Vec<u8>>)> = Vec::new();
        for envelope in envelopes {
            // Nodes only store 16-byte IDs; leave anything else alone.
            let Ok(id) = envelope.id.as_slice().try_into() else {
                continue;
            };
            let key = InboxKey { mailbox, id };
            if self.store.has_inbox(&key)? {
                // Committed before: only the ACK can be missing, maybe lost after mark_acked.
                seen.push(key);
                continue;
            }
            if let Some(event) =
                self.stage_envelope(&key, &envelope.ciphertext, profiles, seal_key)?
            {
                if let Some(group) = receipted_group(&event) {
                    match receipts.iter_mut().find(|(g, _)| g == group) {
                        Some((_, ids)) => ids.push(id.to_vec()),
                        None => receipts.push((group.clone(), vec![id.to_vec()])),
                    }
                }
                events.extend(self.control(nodes, device_key, event, seal_key).await?);
            }
        }
        let acks = self.acks(mailbox, seen)?;
        if !acks.is_empty() {
            let node = nodes.iter_mut().find(|node| node.peer() == at);
            // Always found: `at` is the one of `nodes` that answered the fetch.
            if let Some(node) = node {
                node.ack(device_key, acks.iter().map(|key| key.id.to_vec()).collect())
                    .await?;
                for key in &acks {
                    self.store.mark_acked(key)?;
                }
            }
        }
        // shortcut: a duplicate is not receipted again, its group is unknown here, so a lost
        // receipt costs about 35 resends over 30 days; receipt duplicates once the inbox keeps
        // their group.
        for (group, ids) in receipts {
            for ids in ids.chunks(MAX_RECEIPT_IDS) {
                let receipt = Body::Receipt(Receipt {
                    mailbox: mailbox.to_vec(),
                    ids: ids.to_vec(),
                });
                // A receipt the node refused stays in the outbox for retry_due.
                match self.send_body(nodes, &group, &receipt, seal_key).await {
                    Err(Error::Client(_)) | Ok(()) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(events)
    }

    /// IDs to acknowledge at `mailbox`: the `seen` duplicates and every pending ACK there.
    pub(super) fn acks(
        &self,
        mailbox: [u8; 32],
        mut seen: Vec<InboxKey>,
    ) -> Result<Vec<InboxKey>, Error> {
        for key in self.store.pending_acks()? {
            if key.mailbox == mailbox && !seen.contains(&key) {
                seen.push(key);
            }
        }
        Ok(seen)
    }

    /// Apply one new envelope and commit the result with its pending ACK. A Gap, an unknown
    /// owner or group is neither recorded nor acknowledged, so it is fetched again. A blocked
    /// owner's Welcome is refused; its message advances MLS state, is acknowledged and returns
    /// `None`: no history, no receipt, no token answer.
    pub(super) fn stage_envelope(
        &mut self,
        key: &InboxKey,
        bytes: &[u8],
        profiles: &[VerifiedProfile],
        seal_key: &[u8; 32],
    ) -> Result<Option<Event>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let mut event = match chat::apply(device, &mut self.groups, bytes, profiles) {
            Ok(None) => return Ok(None),
            Ok(Some(event @ (Event::Gap(_) | Event::UnknownOwner(_)))) => return Ok(Some(event)),
            Ok(Some(event)) => event,
            Err(error) => Event::Refused(error.to_string()),
        };
        let mut peer = None;
        if let Event::Joined(id) = &event {
            peer = self.partner(id);
            match &peer {
                None => event = Event::Refused("not a one-to-one conversation".into()),
                Some(owner) if self.store.blocked(owner)? => {
                    event = Event::Refused("blocked owner".into());
                }
                Some(_) => {}
            }
        }
        let blocked = match &event {
            Event::Message { sender, .. } | Event::Interaction { sender, .. } => {
                self.store.blocked(sender)?
            }
            _ => false,
        };
        // Until a reply or a match, a stranger's request is text only: its other bodies are
        // acked and dropped like a blocked owner's, so no token request is answered.
        let blocked = blocked
            || match &event {
                Event::Message {
                    sender,
                    body:
                        Body::Attachment(_)
                        | Body::Forwarded(_)
                        | Body::Presence { .. }
                        | Body::Read { .. }
                        | Body::TokenRequest(_),
                    ..
                } if sender != self.store.owner_peer_id() => matches!(
                    self.store.conversation(sender)?,
                    Conversation::None | Conversation::RequestReceived(_)
                ),
                _ => false,
            };
        // The ledger ignores a replay, so applying before the inbox commit is safe to repeat.
        // shortcut: another device of this owner sent it; it waits for device sync.
        if let Event::Interaction {
            sender,
            device,
            event: interaction,
            ..
        } = &event
        {
            if !blocked && sender != self.store.owner_peer_id() {
                let wall_ms = crate::now() * 1000;
                match self
                    .store
                    .receive_interaction(sender, device, *interaction, wall_ms)
                {
                    Err(crate::store::Error::ClockSkew(_)) => {
                        event = Event::Refused("interaction from the future".into());
                    }
                    applied => {
                        applied?;
                    }
                }
            }
        }
        if let Event::Message {
            sender,
            body: Body::Presence { hlc },
            ..
        } = &event
        {
            if !blocked && sender != self.store.owner_peer_id() {
                let wall_ms = crate::now() * 1000;
                match self.store.receive_presence(sender, *hlc, wall_ms) {
                    Err(crate::store::Error::ClockSkew(_)) => {
                        event = Event::Refused("presence from the future".into());
                    }
                    applied => {
                        applied?;
                    }
                }
            }
        }
        // Only the other owner of a one-to-one chat marks own texts read; a replay changes nothing.
        if let Event::Message {
            group,
            sender,
            body: Body::Read { up_to },
            ..
        } = &event
        {
            if !blocked && self.partner(group).as_ref() == Some(sender) {
                self.store.receive_read(group, sender, *up_to)?;
            }
        }
        let staged = if matches!(event, Event::Refused(_)) {
            // Keep the saved MLS state and reopen it: a refused envelope must change nothing.
            (|| {
                let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
                self.store
                    .stage_inbound(&sealed, key, InboxKind::Refused, b"", None)?;
                self.recover(seal_key).map(|()| true)
            })()
        } else {
            (|| {
                let sealed = device.seal(seal_key)?;
                if !blocked {
                    if let Event::Message {
                        group,
                        sender,
                        device,
                        body,
                    } = &event
                    {
                        if matches!(
                            body,
                            Body::Text { .. } | Body::Edit { .. } | Body::Delete { .. }
                        ) {
                            let verified = VerifiedChatSender {
                                group,
                                author: sender,
                                device,
                            };
                            // A text the request rules drop is acked but not shown.
                            return self
                                .store
                                .stage_chat_inbound(
                                    &sealed,
                                    key,
                                    verified,
                                    body,
                                    crate::now() * 1000,
                                )
                                .map_err(Error::from);
                        }
                    }
                }
                let (kind, payload) = match &event {
                    _ if blocked => (InboxKind::StateChange, Vec::new()),
                    // Token, presence and read bodies are acted on, not shown in history.
                    Event::Message {
                        body:
                            Body::TokenRequest(_)
                            | Body::TokenAnswer(_)
                            | Body::Receipt(_)
                            | Body::Presence { .. }
                            | Body::Read { .. },
                        ..
                    } => (InboxKind::StateChange, Vec::new()),
                    Event::Message {
                        group,
                        sender,
                        body,
                        ..
                    } => (
                        InboxKind::Message,
                        prost::Message::encode_to_vec(&StoredMessage {
                            group: group.clone(),
                            sender: sender.clone(),
                            body: body.encode(),
                        }),
                    ),
                    _ => (InboxKind::StateChange, Vec::new()),
                };
                let joined = match (&event, &peer) {
                    (Event::Joined(id), Some(peer)) => Some((id.as_slice(), peer.as_str())),
                    _ => None,
                };
                self.store
                    .stage_inbound(&sealed, key, kind, &payload, joined)?;
                Ok(true)
            })()
        };
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged.map(|shown| (shown && !blocked).then_some(event))
    }

    /// The one other owner in a joined group.
    // shortcut: a group of only this owner's devices or of three owners is refused; revisit
    // when multi-device sync or group chats exist.
    pub(super) fn partner(&self, id: &[u8]) -> Option<String> {
        let group = self.groups.iter().find(|group| group.id() == id)?;
        let mut others = group.members().ok()?;
        others.retain(|member| member != self.store.owner_peer_id());
        others.sort();
        others.dedup();
        match <[String; 1]>::try_from(others) {
            Ok([peer]) => Some(peer),
            Err(_) => None,
        }
    }
}

/// The group whose sender waits for a receipt of this committed event: any but a receipt, a
/// presence or a read receipt, which leave the outbox once a node accepts them.
fn receipted_group(event: &Event) -> Option<&Vec<u8>> {
    match event {
        Event::Message {
            body: Body::Receipt(_) | Body::Presence { .. } | Body::Read { .. },
            ..
        } => None,
        Event::Message { group, .. }
        | Event::Interaction { group, .. }
        | Event::Joined(group)
        | Event::Commit(group) => Some(group),
        _ => None,
    }
}
