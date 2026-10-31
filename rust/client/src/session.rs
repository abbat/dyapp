//! Durable MLS session. A failed local write discards advanced in-memory state.

use crate::attachment::Attachment;
use crate::chat::{self, Body, Event};
use crate::interactions::{Interaction, Kind};
use crate::scores::{self, Scores};
use crate::store::messages::{CurrentMessage, MessageKey, VerifiedChatSender};
use crate::store::Store;
use crate::{tokens, Node, PreparedEnvelope};
use dyapp_identity::{Identity, SignedRecord};
use dyapp_messaging::mls::{Device, Group};
use dyapp_p2p_net::proto::EnvelopeClass;
use dyapp_profile::VerifiedProfile;

/// A received non-projected message kept in the inbox (for example, an attachment or forward).
/// Text, edit and delete use the current chat projection instead.
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
    /// An edit or delete target is absent or already deleted from this sender's current chat.
    #[error("message is absent or already deleted")]
    UnknownMessage,
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

    /// Open the last committed snapshot or, on the first open, certify a new device for
    /// `identity` with the mailbox key `device_key` and commit its snapshot first.
    pub fn open_or_create(
        mut store: Store,
        identity: &Identity,
        device_key: &Identity,
        seal_key: &[u8; 32],
    ) -> Result<Self, Error> {
        if store.sealed_device()?.is_none() {
            let device = Device::new(identity, device_key.public_key(), 1)?;
            store.stage_outbound(&device.seal(seal_key)?, &[])?;
        }
        Self::open(store, seal_key)
    }

    /// The profile's store, for the calls outside chats such as the profile publish.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The scores of the nodes this session used.
    pub fn scores(&self) -> &Scores {
        &self.scores
    }

    /// Read one current logical message, including its edited or deleted state.
    pub fn chat_message(&self, key: &MessageKey) -> Result<Option<CurrentMessage>, Error> {
        Ok(self.store.chat_message(key)?)
    }

    /// The latest like state in both directions with `peer_id`.
    pub fn likes(&self, peer_id: &str) -> Result<crate::store::Likes, Error> {
        Ok(self.store.likes(peer_id)?)
    }

    /// Where the chat with `peer_id` stands.
    pub fn conversation(&self, peer_id: &str) -> Result<crate::store::Conversation, Error> {
        Ok(self.store.conversation(peer_id)?)
    }

    /// This device's signed certificate, for the owner's profile device list.
    pub fn certificate(&self) -> Result<SignedRecord, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        Ok(device.certificate().clone())
    }

    /// Make `count` single-use KeyPackages, commit the snapshot holding their private keys, then
    /// publish them through `node` under the mailbox key `device_key`. Publish after the profile
    /// that lists this device; a failed publish leaves only unused keys sealed.
    pub async fn publish_key_packages(
        &mut self,
        node: &mut Node,
        device_key: &Identity,
        count: usize,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let packages = (0..count)
            .map(|_| device.key_package(false))
            .collect::<Result<Vec<_>, _>>()?;
        self.store.stage_outbound(&device.seal(seal_key)?, &[])?;
        let owner = self.store.owner_peer_id().to_owned();
        for package in packages {
            node.publish_key_package(device_key, &owner, package, false, 0)
                .await?;
        }
        Ok(())
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
        let interaction = match body {
            Body::Interaction(event) => Some((
                self.partner(group_id).ok_or(Error::UnknownGroup)?,
                device.device_key()?,
                *event,
            )),
            _ => None,
        };
        let presence = match body {
            Body::Presence { hlc } => {
                Some((self.partner(group_id).ok_or(Error::UnknownGroup)?, *hlc))
            }
            _ => None,
        };
        let read = match body {
            Body::Read { up_to } => {
                Some((self.partner(group_id).ok_or(Error::UnknownGroup)?, *up_to))
            }
            _ => None,
        };
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.id() == group_id)
            .ok_or(Error::UnknownGroup)?;
        let staged = (|| {
            let deliveries = chat::message(group, device, body)?;
            let class = match body {
                Body::Interaction(_) | Body::Presence { .. } | Body::Read { .. } => {
                    EnvelopeClass::Signal
                }
                _ => EnvelopeClass::Message,
            };
            let writes = prepare(&self.store, deliveries, class, seal_key)?;
            let sealed = device.seal(seal_key)?;
            if let Some((target, source_device, event)) = interaction {
                self.store
                    .stage_interaction(&target, &source_device, event, &sealed, &writes)?;
            } else if let Some((target, hlc)) = &presence {
                self.store.stage_presence(target, *hlc, &sealed, &writes)?;
            } else if let Some((peer, up_to)) = &read {
                self.store
                    .stage_read(group_id, peer, *up_to, &sealed, &writes)?;
            } else if matches!(body, Body::Receipt(_)) {
                self.store.stage_receipt(&sealed, &writes)?;
            } else if matches!(
                body,
                Body::Text { .. } | Body::Edit { .. } | Body::Delete { .. }
            ) {
                let author = self.store.owner_peer_id().to_owned();
                let source_device = device.device_key()?;
                let sender = VerifiedChatSender {
                    group: group_id,
                    author: &author,
                    device: &source_device,
                };
                self.store.stage_chat_outbound(
                    &sealed,
                    &writes,
                    sender,
                    body,
                    crate::now() * 1000,
                )?;
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

    /// Stage a new text with a caller-visible logical ID before any network request. The returned
    /// exact writes stay in the outbox across an offline period; `retry_due` sends them later.
    pub fn stage_text(
        &mut self,
        group_id: &[u8],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        let writes =
            self.stage_message(group_id, &Body::text_with_id(id, text.to_vec()), seal_key)?;
        Ok((id, writes))
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
        let (_, writes) = self.stage_text(group_id, text, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await
    }

    /// Stage a text to the one other owner in `profiles` (newest held, this owner's among them)
    /// in the pair's canonical group: the joined one with the lowest ID, which both sides pick
    /// once each holds every Welcome. With none yet, a group made from `packages`
    /// ([`chat::key_packages`]) commits its Welcome with the text; the Welcome writes come first.
    pub fn stage_text_to(
        &mut self,
        profiles: &[VerifiedProfile],
        packages: &[(Vec<u8>, Vec<u8>)],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let peer = self.peer_in(profiles)?;
        if let Some(group_id) = self.pair_group(&peer) {
            return self.stage_text(&group_id, text, seal_key);
        }
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let mut id = [0; 16];
        getrandom::fill(&mut id)
            .map_err(|_| Error::Client(crate::Error::Reply("no message randomness")))?;
        let body = Body::text_with_id(id, text.to_vec());
        let staged = (|| {
            let (mut group, welcome) = chat::create(device, packages, profiles)?;
            let class = EnvelopeClass::Message;
            let welcome = prepare(&self.store, welcome, class, seal_key)?;
            let deliveries = chat::message(&mut group, device, &body)?;
            let writes = prepare(&self.store, deliveries, class, seal_key)?;
            let sealed = device.seal(seal_key)?;
            self.store.stage_invite_text(
                &group,
                &peer,
                &sealed,
                &welcome,
                &writes,
                &body,
                &device.device_key()?,
                crate::now() * 1000,
            )?;
            Ok((group, [welcome, writes].concat()))
        })();
        match staged {
            Ok((group, writes)) => {
                self.groups.push(group);
                Ok((id, writes))
            }
            Err(error) => {
                self.device = None;
                self.groups.clear();
                Err(error)
            }
        }
    }

    /// Send a text to the other owner in `profiles` as [`Session::stage_text_to`] stages it,
    /// fetching the KeyPackages only when the pair has no group yet.
    pub async fn send_text_to(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        let peer = self.peer_in(profiles)?;
        let packages = match self.pair_group(&peer) {
            Some(_) => Vec::new(),
            None => self.key_packages(nodes, profiles).await?,
        };
        let (id, writes) = self.stage_text_to(profiles, &packages, text, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(id)
    }

    /// Like, unlike or view the other owner in `profiles` in the pair's canonical group, as
    /// [`Session::interact`] does. A like with no group yet first invites the peer: the Welcome
    /// and the like are staged before either is sent. Without a group nothing else is sent.
    pub async fn interact_with(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
        kind: Kind,
        seal_key: &[u8; 32],
    ) -> Result<Option<Interaction>, Error> {
        let peer = self.peer_in(profiles)?;
        if let Some(group_id) = self.pair_group(&peer) {
            return self.interact(nodes, &group_id, kind, seal_key).await;
        }
        if kind != Kind::Like {
            return Ok(None);
        }
        let packages = self.key_packages(nodes, profiles).await?;
        let Some((event, writes)) = self.stage_first_like(profiles, &packages, seal_key)? else {
            return Ok(None);
        };
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(Some(event))
    }

    /// Stage a Welcome to the other owner in `profiles` for a new pair group made from `packages`
    /// ([`chat::key_packages`]), then a like in that group; returns the like and every write.
    fn stage_first_like(
        &mut self,
        profiles: &[VerifiedProfile],
        packages: &[(Vec<u8>, Vec<u8>)],
        seal_key: &[u8; 32],
    ) -> Result<Option<(Interaction, Vec<PreparedEnvelope>)>, Error> {
        let (group_id, mut writes) = self.stage_invite(profiles, packages, seal_key)?;
        let peer = self.partner(&group_id).ok_or(Error::UnknownGroup)?;
        let wall_ms = crate::now() * 1000;
        let Some(event) = self.store.draft_interaction(&peer, Kind::Like, wall_ms)? else {
            return Ok(None);
        };
        writes.extend(self.stage_message(&group_id, &Body::Interaction(event), seal_key)?);
        Ok(Some((event, writes)))
    }

    /// Stage a Welcome to the other owner in `profiles` for a new pair group made from
    /// `packages`; returns the group ID and the Welcome writes.
    fn stage_invite(
        &mut self,
        profiles: &[VerifiedProfile],
        packages: &[(Vec<u8>, Vec<u8>)],
        seal_key: &[u8; 32],
    ) -> Result<(Vec<u8>, Vec<PreparedEnvelope>), Error> {
        let peer = self.peer_in(profiles)?;
        let peer = profiles
            .iter()
            .find(|profile| profile.peer_id == peer)
            .ok_or(Error::UnknownGroup)?;
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let staged = (|| -> Result<_, Error> {
            let (group, welcome) = chat::create(device, packages, profiles)?;
            let welcome = prepare(&self.store, welcome, EnvelopeClass::Message, seal_key)?;
            self.store
                .stage_group(&group, peer, &device.seal(seal_key)?, &welcome)?;
            Ok((group, welcome))
        })();
        match staged {
            Ok((group, welcome)) => {
                let id = group.id().to_vec();
                self.groups.push(group);
                Ok((id, welcome))
            }
            Err(error) => {
                self.device = None;
                self.groups.clear();
                Err(error)
            }
        }
    }

    /// The peer's verified KeyPackages for a new pair group, from the first node that has them.
    async fn key_packages(
        &mut self,
        nodes: &mut [Node],
        profiles: &[VerifiedProfile],
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Error> {
        let device = self.device.as_ref().ok_or(Error::Invalidated)?;
        let key: [u8; 32] = device
            .device_key()?
            .try_into()
            .map_err(|_| crate::Error::Reply("bad device key"))?;
        let own = crate::mailbox(&key);
        Ok(
            scores::first(&mut self.scores, nodes, Node::peer, async |node| {
                chat::key_packages(node, &own, profiles).await
            })
            .await?,
        )
    }

    /// The one owner in `profiles` other than this one.
    fn peer_in(&self, profiles: &[VerifiedProfile]) -> Result<String, Error> {
        let mut others: Vec<&str> = profiles
            .iter()
            .map(|profile| profile.peer_id.as_str())
            .filter(|peer| *peer != self.store.owner_peer_id())
            .collect();
        others.sort();
        others.dedup();
        match others[..] {
            [peer] => Ok(peer.to_owned()),
            _ => Err(Error::UnknownGroup),
        }
    }

    /// The lowest ID among the joined groups with `peer` alone.
    fn pair_group(&self, peer: &str) -> Option<Vec<u8>> {
        self.groups
            .iter()
            .map(Group::id)
            .filter(|id| self.partner(id).as_deref() == Some(peer))
            .min()
            .map(<[u8]>::to_vec)
    }

    /// Stage an edit of this profile's current text and return its operation ID and exact writes.
    pub fn stage_edit(
        &mut self,
        group_id: &[u8],
        target: [u8; 16],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        self.stage_chat_operation(group_id, target, Some(text), seal_key)
    }

    /// Stage a terminal deletion of this profile's current text and return its operation ID.
    pub fn stage_delete(
        &mut self,
        group_id: &[u8],
        target: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        self.stage_chat_operation(group_id, target, None, seal_key)
    }

    fn stage_chat_operation(
        &mut self,
        group_id: &[u8],
        target: [u8; 16],
        replacement: Option<&[u8]>,
        seal_key: &[u8; 32],
    ) -> Result<([u8; 16], Vec<PreparedEnvelope>), Error> {
        let key = MessageKey {
            group: group_id.to_vec(),
            author: self.store.owner_peer_id().to_owned(),
            id: target,
        };
        let current = self
            .store
            .chat_message(&key)?
            .ok_or(Error::UnknownMessage)?;
        if current.deleted {
            return Err(Error::UnknownMessage);
        }
        let (operation, hlc) = self.store.draft_chat_operation(crate::now() * 1000)?;
        let body = match replacement {
            Some(text) => Body::Edit {
                target,
                operation,
                hlc,
                text: text.to_vec(),
            },
            None => Body::Delete {
                target,
                operation,
                hlc,
            },
        };
        let writes = self.stage_message(group_id, &body, seal_key)?;
        Ok((operation, writes))
    }

    /// Send an edit after staging its exact encrypted writes and local current text.
    pub async fn send_edit(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        target: [u8; 16],
        text: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        let (operation, writes) = self.stage_edit(group_id, target, text, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(operation)
    }

    /// Send a terminal delete after staging its exact encrypted writes and local placeholder.
    pub async fn send_delete(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        target: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<[u8; 16], Error> {
        let (operation, writes) = self.stage_delete(group_id, target, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(operation)
    }

    /// Upload the `blob` from [`crate::attachment::seal`], signed by the owner `identity`, to the
    /// first of `nodes` that takes it, then send its [`Attachment`], with any preview, MIME type
    /// and dimensions set, like a text. Nothing is staged when no node takes the blob.
    pub async fn send_attachment(
        &mut self,
        nodes: &mut [Node],
        identity: &Identity,
        group_id: &[u8],
        attachment: &Attachment,
        blob: &[u8],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        attachment.check()?;
        let mut last = crate::Error::Request("no node".into());
        let mut uploaded = false;
        for node in nodes.iter_mut() {
            let upload = async {
                if !node.attach_media(identity, attachment).await?.is_empty() {
                    node.put_media(&identity.peer_id(), blob.to_vec()).await?;
                }
                Ok(())
            };
            match upload.await {
                Ok(()) => {
                    uploaded = true;
                    break;
                }
                Err(error) => last = error,
            }
        }
        if !uploaded {
            return Err(last.into());
        }
        self.send_body(
            nodes,
            group_id,
            &Body::Attachment(attachment.clone()),
            seal_key,
        )
        .await
    }

    /// Like, unlike or view the other owner of `group_id` and send it as a signal, as
    /// [`Store::draft_interaction`] creates it; `None` when no view is due. Like a text, a failed put stays
    /// in the outbox, and every retry carries the same event.
    pub async fn interact(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        kind: Kind,
        seal_key: &[u8; 32],
    ) -> Result<Option<Interaction>, Error> {
        let target = self.partner(group_id).ok_or(Error::UnknownGroup)?;
        let wall_ms = crate::now() * 1000;
        let Some(event) = self.store.draft_interaction(&target, kind, wall_ms)? else {
            return Ok(None);
        };
        self.send_body(nodes, group_id, &Body::Interaction(event), seal_key)
            .await?;
        Ok(Some(event))
    }

    /// When the app comes to the foreground: send one presence to each allowed match not sent one
    /// in the last hour ([presence](../../docs/design/presence.md)). Unsent ones expire in 2 h.
    pub async fn announce_presence(
        &mut self,
        nodes: &mut [Node],
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_presence(crate::now() * 1000, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await
    }

    fn stage_presence(
        &mut self,
        wall_ms: u64,
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let ids: Vec<Vec<u8>> = self
            .groups
            .iter()
            .map(|group| group.id().to_vec())
            .collect();
        let mut writes = Vec::new();
        for id in ids {
            let Some(target) = self.partner(&id) else {
                continue;
            };
            if let Some(hlc) = self.store.draft_presence(&target, wall_ms)? {
                writes.extend(self.stage_message(&id, &Body::Presence { hlc }, seal_key)?);
            }
        }
        Ok(writes)
    }

    /// When the app shows a chat: send a read receipt for `seen`, the latest text from the other
    /// owner of `group_id` on screen, if one is due ([read receipts](../../docs/design/read-receipts.md)).
    /// Returns whether one was sent.
    pub async fn mark_read(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        seen: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<bool, Error> {
        let writes = self.stage_read(group_id, seen, seal_key)?;
        let sent = !writes.is_empty();
        self.send_staged(nodes, writes, seal_key).await?;
        Ok(sent)
    }

    fn stage_read(
        &mut self,
        group_id: &[u8],
        seen: [u8; 16],
        seal_key: &[u8; 32],
    ) -> Result<Vec<PreparedEnvelope>, Error> {
        let peer = self.partner(group_id).ok_or(Error::UnknownGroup)?;
        if !self.store.draft_read(group_id, &peer, seen)? {
            return Ok(Vec::new());
        }
        self.stage_message(group_id, &Body::Read { up_to: seen }, seal_key)
    }

    async fn send_body(
        &mut self,
        nodes: &mut [Node],
        group_id: &[u8],
        body: &Body,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
        let writes = self.stage_message(group_id, body, seal_key)?;
        self.send_staged(nodes, writes, seal_key).await
    }

    async fn send_staged(
        &mut self,
        nodes: &mut [Node],
        writes: Vec<PreparedEnvelope>,
        seal_key: &[u8; 32],
    ) -> Result<(), Error> {
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
                ..
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
}

/// Each delivery as a write of `class` to its mailbox, with a write token where one is held.
fn prepare(
    store: &Store,
    deliveries: Vec<chat::Delivery>,
    class: EnvelopeClass,
    seal_key: &[u8; 32],
) -> Result<Vec<PreparedEnvelope>, Error> {
    let today = (crate::now() / 86_400) as u32;
    let writes = deliveries
        .into_iter()
        .map(|delivery| {
            tokens::prepare(
                store,
                &delivery.mailbox,
                delivery.mls,
                class,
                today,
                seal_key,
            )
        })
        .collect::<Result<_, _>>()?;
    Ok(writes)
}

mod receive;
#[cfg(test)]
mod tests;
