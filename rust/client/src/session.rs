//! Durable MLS session. A failed local write discards advanced in-memory state.

use crate::chat::{self, Body, Event, Receipt, MAX_RECEIPT_IDS};
use crate::scores::{self, Scores};
use crate::store::{InboxKey, InboxKind, Store};
use crate::{tokens, Node, PreparedEnvelope};
use dyapp_identity::Identity;
use dyapp_messaging::mls::{Device, Group};
use dyapp_p2p_net::proto::EnvelopeClass;
use dyapp_profile::VerifiedProfile;

/// A received message as kept in the inbox for the app's history; protobuf, so fields can be
/// added without a new format.
#[derive(Clone, PartialEq, prost::Message)]
pub struct StoredMessage {
    /// The group ID.
    #[prost(bytes = "vec", tag = "1")]
    pub group: Vec<u8>,
    /// Peer ID of the sender's identity.
    #[prost(string, tag = "2")]
    pub sender: String,
    /// The tagged body, [`chat::Body::encode`].
    #[prost(bytes = "vec", tag = "3")]
    pub body: Vec<u8>,
}

/// Why an outbound session cannot send.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No MLS snapshot has been committed for this profile.
    #[error("no saved MLS session")]
    MissingSnapshot,
    /// A local failure invalidated the advanced MLS state; reopen the saved snapshot.
    #[error("MLS session must be reopened")]
    Invalidated,
    /// The requested conversation is absent from the catalog.
    #[error("unknown MLS conversation")]
    UnknownGroup,
    /// The sealed MLS state could not be opened.
    #[error(transparent)]
    Mls(#[from] dyapp_messaging::mls::Error),
    /// Chat preparation or network delivery failed.
    #[error(transparent)]
    Client(#[from] crate::Error),
    /// The local database failed.
    #[error(transparent)]
    Store(#[from] crate::store::Error),
}

/// One profile's restored MLS device, group handles and durable mailbox queue.
pub struct Session {
    store: Store,
    device: Option<Device>,
    groups: Vec<Group>,
    scores: Scores,
}

impl Session {
    /// Open the last committed snapshot. The app supplies its protected sealing key.
    pub fn open(store: Store, seal_key: &[u8; 32]) -> Result<Self, Error> {
        let sealed = store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let device = Device::open(&sealed, seal_key)?;
        let groups = store
            .load_groups(&device)?
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        Ok(Self {
            store,
            device: Some(device),
            groups,
            scores: Scores::default(),
        })
    }

    /// The scores of the nodes this session used.
    pub fn scores(&self) -> &Scores {
        &self.scores
    }

    /// Block `contact` (hex Peer ID): record it, sign it no more tokens, and revoke those it holds
    /// through the first node that answers. The block stays if the revoke fails; call again to
    /// repeat it.
    pub async fn block(
        &mut self,
        nodes: &mut [Node],
        device_key: &Identity,
        contact: &str,
    ) -> Result<(), Error> {
        self.store.set_blocked(contact, true)?;
        let keys = self
            .store
            .revoke_issued(contact, (crate::now() / 86_400) as u32)?;
        scores::first(&mut self.scores, nodes, Node::peer, async |node| {
            node.revoke_tokens(device_key, &keys).await
        })
        .await?;
        Ok(())
    }

    /// Unblock `contact`. Revoked keys stay revoked; it gets tokens again for days it asks anew.
    pub fn unblock(&mut self, contact: &str) -> Result<(), Error> {
        Ok(self.store.set_blocked(contact, false)?)
    }

    /// Reopen the committed snapshot after a local failure; never reuse the advanced handles.
    pub fn recover(&mut self, seal_key: &[u8; 32]) -> Result<(), Error> {
        self.device = None;
        self.groups.clear();
        let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
        let device = Device::open(&sealed, seal_key)?;
        self.groups = self
            .store
            .load_groups(&device)?
            .into_iter()
            .map(|(g, _)| g)
            .collect();
        self.device = Some(device);
        Ok(())
    }

    /// Commit the new MLS state and exact outbound bytes before the first network request.
    fn stage_message(
        &mut self,
        group_id: &[u8],
        body: &Body,
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::message(group, device, body)?;
            let today = (crate::now() / 86_400) as u32;
            let class = match body {
                Body::Interaction(_) => EnvelopeClass::Signal,
                _ => EnvelopeClass::Message,
            };
            let writes = deliveries
                .into_iter()
                .map(|delivery| {
                    tokens::prepare(
                        &self.store,
                        &delivery.mailbox,
                        delivery.mls,
                        class,
                        today,
                        seal_key,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let sealed = device.seal(seal_key)?;
            if matches!(body, Body::Receipt(_)) {
                self.store.stage_receipt(&sealed, &writes)?;
            } else {
                self.store.stage_outbound(&sealed, &writes)?;
            }
            Ok(writes)
        })();
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged
    }

    /// Send a text only after the new MLS state and every signed envelope are durable. Each write
    /// carries this device's token for today when it holds one; a write refused for its token is
    /// replaced in the outbox as in [`tokens::send`] and put again.
    pub async fn send_message(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        self.send_body(nodes, group_id, &Body::Text(text.to_vec()), seal_key)
            .await
    }

    async fn send_body(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        body: &Body,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_message(group_id, body, seal_key)?;
        let now_ms = crate::now() as i64 * 1000;
        for write in writes {
            self.deliver(nodes, write, now_ms, seal_key).await?;
        }
        Ok(())
    }

    /// Put every outbox write due by `now_ms` again, as the app does after a restart or a lost
    /// connection; a failed one waits longer each time, up to an hour. An accepted write is put
    /// again until its receipt, 1 h later and then up to a day apart, for 30 days.
    pub async fn retry_due(
        &mut self,
        nodes: &mut [Node],
        now_ms: i64,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        self.store.expire_outbound(now_ms)?;
        for write in self.store.due_outbound(now_ms)? {
            match self.deliver(nodes, write.clone(), now_ms, seal_key).await {
                Err(Error::Client(_)) => match self.store.schedule_retry(&write, now_ms) {
                    // A token swap replaced the row, so it is due again at once.
                    Err(crate::store::Error::OutboxConflict) => {}
                    other => other?,
                },
                other => other?,
            }
        }
        Ok(())
    }

    /// Put `write` through the first of `nodes` by score that accepts it, the node that accepted
    /// it last tried last, swapping a refused tokened copy for its fallback in the outbox. Every
    /// node that fails is scored; the last error is returned when all fail.
    async fn deliver(
        &mut self,
        nodes: &mut [Node],
        mut write: PreparedEnvelope,
        now_ms: i64,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let today = (now_ms / 86_400_000) as u32;
        let now = (now_ms / 1000) as u64;
        let peers: Vec<_> = nodes.iter().map(Node::peer).collect();
        let mut order = self.scores.ranked(&peers, now);
        if let Some(by) = self.store.accepted_by(&write)? {
            if let Some(at) = order.iter().position(|&i| peers[i].to_string() == by) {
                let i = order.remove(at);
                order.push(i);
            }
        }
        let mut last = crate::Error::Status(dyapp_p2p_net::proto::Status::NotFound);
        for i in order {
            loop {
                match nodes[i].put_prepared(&write).await {
                    Ok(()) => {
                        let peer = peers[i].to_string();
                        return Ok(self.store.accepted(&write, now_ms, &peer)?);
                    }
                    Err(crate::Error::Status(status)) => {
                        match tokens::fallback(&self.store, &write, status, today, seal_key)? {
                            Some(next) => {
                                self.store.replace_outbound(&write, &next)?;
                                write = next;
                                continue;
                            }
                            None => last = crate::Error::Status(status),
                        }
                    }
                    Err(error) => last = error,
                }
                break;
            }
            if scores::faulty(&last) {
                self.scores.fault(peers[i], now);
            }
        }
        Err(last.into())
    }

    /// Ask every group's other devices for the write tokens this device is missing; the app calls
    /// it once a day. A join and a contact's request ask by themselves.
    pub async fn refill(&mut self, nodes: &mut [Node], seal_key: &[u8; 32]) -> Result<(), Error> {
        let ids = self
            .groups
            .iter()
            .map(|g| g.id().to_vec())
            .collect::<Vec<_>>();
        for id in ids {
            self.request_tokens(nodes, &id, seal_key).await?;
        }
        Ok(())
    }

    async fn request_tokens(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let group = self
            .groups
            .iter()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let mailboxes = group
            .peer_device_keys()?
            .iter()
            .filter_map(|key| {
                <&[u8; 32]>::try_from(key.as_slice())
                    .ok()
                    .map(crate::mailbox)
            })
            .collect::<Vec<_>>();
        let today = (crate::now() / 86_400) as u32;
        match tokens::request(&mut self.store, &mailboxes, today, seal_key)? {
            Some(body) => self.send_body(nodes, group_id, &body, seal_key).await,
            None => Ok(()),
        }
    }

    /// Act on a committed token body or join instead of showing it: store a contact's tokens, or
    /// answer its request and ask back; ask after a join. Returns the events the app sees.
    // shortcut: a crash between the inbox commit and this step loses the answer or the stored
    // tokens; the next refill asks again.
    async fn control(
        &mut self,
        nodes: &mut [Node],
        device_key: &Identity,
        event: Event,
        seal_key: &[u8; 32],
    ) -> Result<Option<Event>, Error> {
        let today = (crate::now() / 86_400) as u32;
        match event {
            Event::Message {
                group,
                sender,
                body: Body::TokenRequest(request),
            } => {
                for answer in tokens::answer(&self.store, device_key, &sender, &request, today)? {
                    self.send_body(nodes, &group, &answer, seal_key).await?;
                }
                self.request_tokens(nodes, &group, seal_key).await?;
                Ok(None)
            }
            Event::Message {
                body: Body::TokenAnswer(answer),
                ..
            } => {
                tokens::accept(&self.store, &answer, seal_key)?;
                Ok(None)
            }
            // IDs are random per write, so only their recipient can name them; a repeat or a
            // stranger's ID drops nothing.
            Event::Message {
                body: Body::Receipt(receipt),
                ..
            } => {
                let mailbox = receipt.mailbox.as_slice().try_into().unwrap_or([0; 32]);
                let ids = receipt
                    .ids
                    .iter()
                    .filter_map(|id| id.as_slice().try_into().ok())
                    .collect::<Vec<_>>();
                let now = crate::now();
                for peer in self.store.receipted(&mailbox, &ids)?.1 {
                    if let Ok(peer) = peer.parse() {
                        self.scores.fault(peer, now);
                    }
                }
                Ok(None)
            }
            Event::Joined(group) => {
                self.request_tokens(nodes, &group, seal_key).await?;
                Ok(Some(Event::Joined(group)))
            }
            event => Ok(Some(event)),
        }
    }

    /// Reopen exact signed bytes for the retry worker after a crash or failed node request.
    pub fn due_outbound(&self, now_ms: i64) -> Result<Vec<PreparedEnvelope>, Error> {
        Ok(self.store.due_outbound(now_ms)?)
    }

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
            node.expect("fetched from one of nodes")
                .ack(device_key, acks.iter().map(|key| key.id.to_vec()).collect())
                .await?;
            for key in &acks {
                self.store.mark_acked(key)?;
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
    fn acks(&self, mailbox: [u8; 32], mut seen: Vec<InboxKey>) -> Result<Vec<InboxKey>, Error> {
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
    fn stage_envelope(
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
            Event::Message { sender, .. } => self.store.blocked(sender)?,
            _ => false,
        };
        let staged = if matches!(event, Event::Refused(_)) {
            // Keep the saved MLS state and reopen it: a refused envelope must change nothing.
            (|| {
                let sealed = self.store.sealed_device()?.ok_or(Error::MissingSnapshot)?;
                self.store
                    .stage_inbound(&sealed, key, InboxKind::Refused, b"", None)?;
                self.recover(seal_key)
            })()
        } else {
            (|| {
                let (kind, payload) = match &event {
                    _ if blocked => (InboxKind::StateChange, Vec::new()),
                    // Token bodies are acted on, not shown in history.
                    Event::Message {
                        body: Body::TokenRequest(_) | Body::TokenAnswer(_) | Body::Receipt(_),
                        ..
                    } => (InboxKind::StateChange, Vec::new()),
                    Event::Message {
                        group,
                        sender,
                        body,
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
                let sealed = device.seal(seal_key)?;
                self.store
                    .stage_inbound(&sealed, key, kind, &payload, joined)?;
                Ok(())
            })()
        };
        if staged.is_err() {
            self.device = None;
            self.groups.clear();
        }
        staged.map(|()| (!blocked).then_some(event))
    }

    /// The one other owner in a joined group.
    // shortcut: a group of only this owner's devices or of three owners is refused; revisit
    // when multi-device sync or group chats exist.
    fn partner(&self, id: &[u8]) -> Option<String> {
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

/// The group whose sender waits for a receipt of this committed event: any but a receipt.
fn receipted_group(event: &Event) -> Option<&Vec<u8>> {
    match event {
        Event::Message {
            body: Body::Receipt(_),
            ..
        } => None,
        Event::Message { group, .. } | Event::Joined(group) | Event::Commit(group) => Some(group),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Session, StoredMessage};
    use crate::chat::{self, Body, Event};
    use crate::store::{InboxKey, InboxKind, Store};
    use dyapp_identity::Identity;
    use dyapp_messaging::mls::Device;
    use dyapp_profile::Profile;

    fn person() -> (Identity, Device, dyapp_profile::VerifiedProfile) {
        let identity = Identity::generate();
        let device = Device::new(&identity, Identity::generate().public_key(), 1).unwrap();
        let profile = Profile {
            version: 1,
            devices: vec![device.certificate().clone()],
            ..Default::default()
        }
        .sign(&identity);
        (identity, device, dyapp_profile::verify(&profile).unwrap())
    }

    #[test]
    fn message_is_staged_before_network_and_reopens_exact_bytes() {
        let (alice_id, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let (group, _) = alice
            .create_group(
                &[bob.key_package(false).unwrap()],
                &[alice_profile, bob_profile.clone()],
            )
            .unwrap();
        let group_id = group.id().to_vec();
        let seal_key = [3; 32];
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-send-session-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let mut store = Store::open(&path, &alice_id).unwrap();
        store
            .stage_group(&group, &bob_profile, &alice.seal(&seal_key).unwrap(), &[])
            .unwrap();
        let mut session = Session::open(store, &seal_key).unwrap();
        let writes = session
            .stage_message(&group_id, &Body::Text(b"hello".to_vec()), &seal_key)
            .unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(
            session.due_outbound(0).unwrap()[0].as_bytes(),
            writes[0].as_bytes()
        );
        drop(session);
        let session = Session::open(Store::open(&path, &alice_id).unwrap(), &seal_key).unwrap();
        assert_eq!(
            session.due_outbound(0).unwrap()[0].as_bytes(),
            writes[0].as_bytes()
        );
    }

    fn temp_path() -> std::path::PathBuf {
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        std::env::temp_dir().join(format!(
            "dyapp-receive-session-{:032x}.db",
            u128::from_be_bytes(random)
        ))
    }

    fn key(id: u8) -> InboxKey {
        InboxKey {
            mailbox: [9; 32],
            id: [id; 16],
        }
    }

    /// Bob's session over a fresh store, invited by Alice to `group` with `welcome`.
    struct Invite {
        session: Session,
        bob_id: Identity,
        alice_id: Identity,
        alice: Device,
        group: dyapp_messaging::mls::Group,
        welcome: Vec<u8>,
        profiles: [dyapp_profile::VerifiedProfile; 2],
    }

    fn bob_invited(path: &std::path::Path, seal_key: &[u8; 32]) -> Invite {
        let (alice_id, alice, alice_profile) = person();
        let (bob_id, bob, bob_profile) = person();
        let profiles = [alice_profile, bob_profile];
        let (group, welcome) = alice
            .create_group(&[bob.key_package(false).unwrap()], &profiles)
            .unwrap();
        let mut store = Store::open(path, &bob_id).unwrap();
        store
            .stage_outbound(&bob.seal(seal_key).unwrap(), &[])
            .unwrap();
        Invite {
            session: Session::open(store, seal_key).unwrap(),
            bob_id,
            alice_id,
            alice,
            group,
            welcome,
            profiles,
        }
    }

    #[test]
    fn received_envelopes_commit_before_ack_and_survive_restart() {
        let path = temp_path();
        let seal_key = [5; 32];
        let Invite {
            mut session,
            bob_id,
            alice_id,
            alice,
            mut group,
            welcome,
            profiles,
        } = bob_invited(&path, &seal_key);
        assert!(
            session
                .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
                .unwrap()
                == Some(Event::Joined(group.id().to_vec()))
        );
        assert_eq!(
            session.store.known_groups().unwrap(),
            [(group.id().to_vec(), alice_id.peer_id())]
        );
        let hi = chat::message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
        let payload: Vec<u8> = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT `payload` FROM `inbox` WHERE `envelope_id` = ?1",
                [[2_u8; 16].as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            <StoredMessage as prost::Message>::decode(payload.as_slice()).unwrap()
                == StoredMessage {
                    group: group.id().to_vec(),
                    sender: alice_id.peer_id(),
                    body: Body::Text(b"hi".to_vec()).encode(),
                }
        );
        drop(session);

        // Restarted before the ACK: the joined group and both pending ACKs are back.
        let mut session = Session::open(Store::open(&path, &bob_id).unwrap(), &seal_key).unwrap();
        assert!(session.store.has_inbox(&key(2)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
        let again = chat::message(&mut group, &alice, &Body::Text(b"again".to_vec())).unwrap();
        let Some(Event::Message { body, .. }) = session
            .stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key)
            .unwrap()
        else {
            panic!("expected the next message");
        };
        assert!(body == Body::Text(b"again".to_vec()));

        // A duplicate seen after its ACK is acknowledged again; another mailbox is not.
        for id in [1, 2, 3] {
            assert!(session.store.mark_acked(&key(id)).unwrap());
        }
        let other = InboxKey {
            mailbox: [8; 32],
            id: [4; 16],
        };
        session
            .store
            .stage_inbound(b"sealed", &other, InboxKind::StateChange, b"", None)
            .unwrap();
        assert_eq!(session.acks([9; 32], vec![key(2)]).unwrap(), [key(2)]);
    }

    #[test]
    fn a_blocked_owner_is_not_joined_and_its_messages_are_acked_unseen() {
        let seal_key = [7; 32];
        let invite = bob_invited(&temp_path(), &seal_key);
        let mut session = invite.session;
        session
            .store
            .set_blocked(&invite.alice_id.peer_id(), true)
            .unwrap();
        assert!(matches!(
            session.stage_envelope(&key(1), &invite.welcome, &invite.profiles, &seal_key),
            Ok(Some(Event::Refused(_)))
        ));
        assert!(session.store.known_groups().unwrap().is_empty());

        let Invite {
            mut session,
            alice_id,
            alice,
            mut group,
            welcome,
            profiles,
            ..
        } = bob_invited(&temp_path(), &seal_key);
        session
            .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
            .unwrap();
        session
            .store
            .set_blocked(&alice_id.peer_id(), true)
            .unwrap();
        let hi = chat::message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        assert!(session
            .stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key)
            .unwrap()
            .is_none());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1), key(2)]);
        // The ratchet moved on: after an unblock the next message still opens.
        session
            .store
            .set_blocked(&alice_id.peer_id(), false)
            .unwrap();
        let again = chat::message(&mut group, &alice, &Body::Text(b"again".to_vec())).unwrap();
        assert!(matches!(
            session.stage_envelope(&key(3), &again[0].mls, &profiles, &seal_key),
            Ok(Some(Event::Message { .. }))
        ));
    }

    #[test]
    fn failed_receive_commit_neither_acks_nor_loses_the_envelope() {
        let path = temp_path();
        let seal_key = [6; 32];
        let Invite {
            mut session,
            alice,
            mut group,
            welcome,
            profiles,
            ..
        } = bob_invited(&path, &seal_key);
        session
            .stage_envelope(&key(1), &welcome, &profiles, &seal_key)
            .unwrap();
        let hi = chat::message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
        )
        .unwrap();
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Err(Error::Store(_))
        ));
        assert!(!session.store.has_inbox(&key(2)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
        assert!(matches!(
            session.stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key),
            Err(Error::Invalidated)
        ));

        db.execute_batch("DROP TRIGGER `fail_mls`").unwrap();
        session.recover(&seal_key).unwrap();
        let Some(Event::Message { body, .. }) = session
            .stage_envelope(&key(2), &hi[0].mls, &profiles, &seal_key)
            .unwrap()
        else {
            panic!("expected the message after recovery");
        };
        assert!(body == Body::Text(b"hi".to_vec()));
    }

    #[test]
    fn refused_envelope_is_marked_without_changing_mls_state() {
        let path = temp_path();
        let seal_key = [7; 32];
        let Invite {
            mut session,
            profiles,
            ..
        } = bob_invited(&path, &seal_key);
        let sealed = session.store.sealed_device().unwrap();
        assert!(matches!(
            session.stage_envelope(&key(1), b"not mls", &profiles, &seal_key),
            Ok(Some(Event::Refused(_)))
        ));
        assert!(session.store.has_inbox(&key(1)).unwrap());
        assert_eq!(session.acks([9; 32], vec![]).unwrap(), [key(1)]);
        assert_eq!(session.store.sealed_device().unwrap(), sealed);
        assert!(session.device.is_some());
    }

    #[test]
    fn failed_store_write_invalidates_session_until_recovery() {
        let (alice_id, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let (group, _) = alice
            .create_group(
                &[bob.key_package(false).unwrap()],
                &[alice_profile, bob_profile.clone()],
            )
            .unwrap();
        let group_id = group.id().to_vec();
        let seal_key = [4; 32];
        let mut random = [0; 16];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "dyapp-send-session-{:032x}.db",
            u128::from_be_bytes(random)
        ));
        let mut store = Store::open(&path, &alice_id).unwrap();
        let sealed = alice.seal(&seal_key).unwrap();
        store
            .stage_group(&group, &bob_profile, &sealed, &[])
            .unwrap();
        let mut session = Session::open(store, &seal_key).unwrap();
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER `fail_mls` BEFORE UPDATE ON `mls_state`
             BEGIN SELECT RAISE(ABORT, 'test failure'); END",
            )
            .unwrap();
        assert!(matches!(
            session.stage_message(&group_id, &Body::Text(b"hello".to_vec()), &seal_key),
            Err(Error::Store(_))
        ));
        assert!(matches!(
            session.stage_message(&group_id, &Body::Text(b"retry".to_vec()), &seal_key),
            Err(Error::Invalidated)
        ));
        assert!(session.due_outbound(0).unwrap().is_empty());
        assert_eq!(session.store.sealed_device().unwrap(), Some(sealed));
        session.recover(&seal_key).unwrap();
        assert!(session.device.is_some());
    }
}
