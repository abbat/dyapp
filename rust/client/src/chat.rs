//! Chats as MLS groups delivered through device mailboxes. Every Welcome, commit and message is
//! MLS bytes in an envelope, one envelope per recipient device, so a node stores only
//! ciphertext. A message's plaintext is padded to the next power of two from 1 KiB before
//! encryption, so its size tells little about its content; a like, unlike, view, presence or read
//! receipt pads from 256 bytes so its envelope fits the node's 1 KiB signal limit.
//!
//! Calls that change MLS state return the [`Delivery`] list instead of sending it: seal and
//! store the device state ([`Device::seal`]) first, then [`send`] it. Likewise [`receive`]
//! returns the envelope ids to [`Node::ack`] once the new state is stored.

use crate::attachment::Attachment;
use crate::interactions::Interaction;
use crate::scores::Scores;
use crate::{mailbox, Envelope, Error, Node};
use dyapp_identity::{Identity, SignedRecord};
use dyapp_messaging::mls::{self, Device, Group, Received};
use dyapp_profile::{is_clean_text, DeviceCertificate, VerifiedProfile, MAX_FIELD_CHARS};
use prost::Message as _;

/// The smallest padded plaintext.
pub const MIN_PADDED_BYTES: usize = 1024;
/// The smallest padded interaction plaintext.
pub const SIGNAL_PADDED_BYTES: usize = 256;
/// Largest signed payload of a signal envelope the node accepts.
pub const MAX_SIGNAL_BYTES: usize = 1024;
/// Maximum text bytes in a chat text or edit, before MLS padding.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// Initial private contact-note limit, in Unicode code points.
pub const MAX_NOTE_CHARS: usize = 2_000;

/// Plain private note text: clean NFC lines, retaining case and LF line breaks.
pub fn valid_note(note: &str) -> bool {
    note.chars().count() <= MAX_NOTE_CHARS && note.split('\n').all(is_clean_text)
}
const MESSAGE_ID_BYTES: usize = 16;
const PEER_ID_CHARS: usize = 64;
const OPERATION_HEADER_BYTES: usize = 2 * MESSAGE_ID_BYTES + 8;
// The big-endian plaintext length that starts a padded plaintext.
const LENGTH_BYTES: usize = 4;

/// MLS bytes for one device mailbox.
#[derive(Clone, PartialEq, Eq)]
pub struct Delivery {
    /// The recipient device's mailbox address.
    pub mailbox: Vec<u8>,
    /// A Welcome, commit or application message.
    pub mls: Vec<u8>,
}

/// Shows the size only: the mailbox address names a contact.
impl std::fmt::Debug for Delivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delivery")
            .field("mls_len", &self.mls.len())
            .finish_non_exhaustive()
    }
}

/// What a fetched envelope carried.
#[derive(PartialEq, Eq)]
pub enum Event {
    /// This device joined the group with this ID.
    Joined(Vec<u8>),
    /// A member's message.
    Message {
        /// The group ID.
        group: Vec<u8>,
        /// Peer ID of the sender's identity.
        sender: String,
        /// MLS-authenticated sender device key.
        device: Vec<u8>,
        /// What the message carries; never an interaction.
        body: Body,
    },
    /// A member's like, unlike or view.
    Interaction {
        /// The group ID.
        group: Vec<u8>,
        /// Peer ID of the sender's identity.
        sender: String,
        /// The sending device's mailbox key, which orders concurrent events.
        device: Vec<u8>,
        /// The decoded event.
        event: Interaction,
    },
    /// A commit moved the group with this ID to its next epoch.
    Commit(Vec<u8>),
    /// A message from a later epoch of the group with this ID: a commit is missing, maybe hidden
    /// by the node. The envelope stays in the mailbox; run [`cross_check`].
    Gap(Vec<u8>),
    /// A Welcome names a member of the owner with this Peer ID, whose profile was not given. The
    /// envelope stays in the mailbox: fetch the profile and receive again.
    UnknownOwner(String),
    /// The envelope was refused and acked: no group took it.
    Refused(String),
}

/// A local decision about a peer that the owner's devices share.
#[derive(Clone, PartialEq, Eq)]
pub enum PeerChange {
    /// Blocked or unblocked.
    Blocked(bool),
    /// Muted or unmuted.
    Muted(bool),
    /// Passed in search or the pass undone.
    Passed(bool),
    /// The local contact name; empty removes it.
    Name(String),
    /// Privately saved for later, without sending a like.
    Favorite(bool),
    /// Private contact note; empty text clears it on the owner's devices.
    Note(String),
}

impl PeerChange {
    /// Whether the Peer ID and any private text fit their respective limits.
    pub fn valid(&self, peer: &str) -> bool {
        let name_fits = match self {
            Self::Name(name) => name.chars().count() <= MAX_FIELD_CHARS && is_clean_text(name),
            Self::Note(note) => valid_note(note),
            _ => true,
        };
        name_fits && peer.len() == PEER_ID_CHARS && peer.bytes().all(|c| c.is_ascii_hexdigit())
    }
}

/// What an application message carries: the first plaintext byte names the kind, so a chat text
/// is never read as an interaction.
#[derive(Clone, PartialEq, Eq)]
pub enum Body {
    /// User text with its logical ID. Legacy tag 1 has no time; tag 12 carries an HLC.
    Text {
        /// Stable logical ID shared across recipient devices.
        id: [u8; 16],
        /// Original sender HLC; absent for tag 1.
        hlc: Option<u64>,
        /// The current text bytes.
        text: Vec<u8>,
    },
    /// A like, unlike or view, tag 2.
    Interaction(Interaction),
    /// A contact device asks the owner for WriteTokens, tag 3.
    TokenRequest(TokenRequest),
    /// The owner's signed WriteTokens, tag 4.
    TokenAnswer(TokenAnswer),
    /// A device received writes in its mailbox, tag 5.
    Receipt(Receipt),
    /// An encrypted blob with a message-scoped logical ID and original HLC, tag 6.
    Attachment(AttachmentMessage),
    /// A text or attachment the sender forwards, tag 7. It names no original author: the MLS
    /// sender is the only sender, so forwarded words never pass for another member's.
    Forwarded(Box<Body>),
    /// Replace the author's earlier text, tag 8.
    Edit {
        /// Logical ID of the original text.
        target: [u8; 16],
        /// Stable ID of this operation.
        operation: [u8; 16],
        /// Hybrid logical timestamp.
        hlc: u64,
        /// Replacement text bytes.
        text: Vec<u8>,
    },
    /// Remove the author's earlier text, tag 9.
    Delete {
        /// Logical ID of the original text.
        target: [u8; 16],
        /// Stable ID of this operation.
        operation: [u8; 16],
        /// Hybrid logical timestamp.
        hlc: u64,
    },
    /// Coarse "active recently" signal to a match, tag 10 ([presence](../../docs/design/presence.md)).
    Presence {
        /// Hybrid logical timestamp.
        hlc: u64,
    },
    /// The reader saw the author's texts up to this logical ID, tag 11
    /// ([read receipts](../../docs/design/read-receipts.md)).
    Read {
        /// Logical ID of the latest text seen.
        up_to: [u8; 16],
    },
    /// Another device of this owner changed a local decision about `peer`, tag 13.
    PeerChange {
        /// The Peer ID the decision is about.
        peer: String,
        /// HLC of the change: the newest wins.
        hlc: u64,
        /// The decision.
        change: PeerChange,
    },
}

/// One send of a media descriptor; repeated sends of the same blob remain distinct messages.
#[derive(Clone, PartialEq, Eq)]
pub struct AttachmentMessage {
    /// Logical ID shared across deliveries to recipient devices.
    pub id: [u8; 16],
    /// Authenticated sender HLC for local history ordering.
    pub hlc: u64,
    /// The media descriptor, reusable independently of this message.
    pub attachment: Attachment,
}

mod attachment_wire;
use attachment_wire::AttachmentWire;
/// Bounded sticker payload codec; outer message integration is not enabled yet.
pub mod sticker;

/// The most write IDs one receipt names, above a node's fetch page.
pub const MAX_RECEIPT_IDS: usize = 256;

/// Envelope IDs a device committed from its mailbox, so the sender drops them from its outbox.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Receipt {
    /// The receiving device's 32-byte mailbox address.
    #[prost(bytes = "vec", tag = "1")]
    pub mailbox: Vec<u8>,
    /// One to [`MAX_RECEIPT_IDS`] 16-byte envelope IDs.
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub ids: Vec<Vec<u8>>,
}

/// The most asks or tokens in one body: 30 days for each of 64 owner devices.
pub const MAX_TOKENS: usize = 30 * 64;

/// One day's write token wanted for one owner device mailbox.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TokenAsk {
    /// The owner device's 32-byte mailbox address.
    #[prost(bytes = "vec", tag = "1")]
    pub mailbox: Vec<u8>,
    /// The UTC day, Unix seconds / 86400.
    #[prost(uint32, tag = "2")]
    pub epoch: u32,
    /// The contact's 32-byte public token key for that day.
    #[prost(bytes = "vec", tag = "3")]
    pub token_key: Vec<u8>,
}

/// Write tokens a contact device asks for.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TokenRequest {
    /// One to [`MAX_TOKENS`] asks.
    #[prost(message, repeated, tag = "1")]
    pub asks: Vec<TokenAsk>,
}

/// The owner device's answer to a [`TokenRequest`].
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct TokenAnswer {
    /// One to [`MAX_TOKENS`] `SignedRecord` bytes, each a `WriteToken` under
    /// `Domain::WriteToken`; the receiver verifies them.
    #[prost(bytes = "vec", repeated, tag = "1")]
    pub signed_tokens: Vec<Vec<u8>>,
}

impl Body {
    /// Media message carried directly or inside a forward.
    pub fn attachment_message(&self) -> Option<&AttachmentMessage> {
        match self {
            Self::Attachment(message) => Some(message),
            Self::Forwarded(inner) => match inner.as_ref() {
                Self::Attachment(message) => Some(message),
                _ => None,
            },
            _ => None,
        }
    }

    /// Makes a text body with a caller-owned logical ID, shared by all device deliveries.
    pub fn text_with_id(id: [u8; 16], text: Vec<u8>) -> Self {
        Self::Text {
            id,
            hlc: None,
            text,
        }
    }

    /// The tag byte, then the bounded body bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Text { id, hlc, text } => match hlc {
                Some(hlc) => [&[12], id.as_slice(), &hlc.to_be_bytes(), text.as_slice()].concat(),
                None => [&[1], id.as_slice(), text.as_slice()].concat(),
            },
            Self::Interaction(event) => [&[2], event.encode().as_slice()].concat(),
            Self::TokenRequest(request) => [&[3], request.encode_to_vec().as_slice()].concat(),
            Self::TokenAnswer(answer) => [&[4], answer.encode_to_vec().as_slice()].concat(),
            Self::Receipt(receipt) => [&[5], receipt.encode_to_vec().as_slice()].concat(),
            Self::Attachment(message) => {
                let wire = AttachmentWire::from(message);
                [&[6], wire.encode_to_vec().as_slice()].concat()
            }
            Self::Forwarded(body) => [&[7], body.encode().as_slice()].concat(),
            Self::Edit {
                target,
                operation,
                hlc,
                text,
            } => [
                &[8],
                target.as_slice(),
                operation.as_slice(),
                &hlc.to_be_bytes(),
                text.as_slice(),
            ]
            .concat(),
            Self::Delete {
                target,
                operation,
                hlc,
            } => [
                &[9],
                target.as_slice(),
                operation.as_slice(),
                &hlc.to_be_bytes(),
            ]
            .concat(),
            Self::Presence { hlc } => [&[10], &hlc.to_be_bytes()[..]].concat(),
            Self::Read { up_to } => [&[11], up_to.as_slice()].concat(),
            Self::PeerChange { peer, hlc, change } => {
                let (kind, value) = match change {
                    PeerChange::Blocked(on) => (1, vec![u8::from(*on)]),
                    PeerChange::Muted(on) => (2, vec![u8::from(*on)]),
                    PeerChange::Passed(on) => (3, vec![u8::from(*on)]),
                    PeerChange::Name(name) => (4, name.clone().into_bytes()),
                    PeerChange::Favorite(on) => (5, vec![u8::from(*on)]),
                    PeerChange::Note(note) => (6, note.clone().into_bytes()),
                };
                let head = [&[13, kind], hlc.to_be_bytes().as_slice()].concat();
                [head.as_slice(), peer.as_bytes(), value.as_slice()].concat()
            }
        }
    }

    /// Refuses an unknown or missing tag, a malformed interaction, a token body that is empty or
    /// too long, a token, receipt or attachment field of the wrong length, and a forward of
    /// anything but a text or attachment.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        match bytes.split_first() {
            Some((1, body)) => {
                let (id, text) = body
                    .split_first_chunk::<MESSAGE_ID_BYTES>()
                    .ok_or(Error::Reply("bad text"))?;
                if text.len() > MAX_TEXT_BYTES {
                    return Err(Error::Reply("bad text"));
                }
                Ok(Self::Text {
                    id: *id,
                    hlc: None,
                    text: text.to_vec(),
                })
            }
            Some((13, body)) => peer_change(body).ok_or(Error::Reply("bad peer change")),
            Some((2, event)) => Interaction::decode(event)
                .map(Self::Interaction)
                .map_err(|_| Error::Reply("bad interaction")),
            Some((3, request)) => TokenRequest::decode(request)
                .ok()
                .filter(|r| {
                    (1..=MAX_TOKENS).contains(&r.asks.len())
                        && r.asks
                            .iter()
                            .all(|a| a.mailbox.len() == 32 && a.token_key.len() == 32)
                })
                .map(Self::TokenRequest)
                .ok_or(Error::Reply("bad token request")),
            Some((4, answer)) => TokenAnswer::decode(answer)
                .ok()
                .filter(|a| {
                    (1..=MAX_TOKENS).contains(&a.signed_tokens.len())
                        && a.signed_tokens.iter().all(|t| !t.is_empty())
                })
                .map(Self::TokenAnswer)
                .ok_or(Error::Reply("bad token answer")),
            Some((5, receipt)) => Receipt::decode(receipt)
                .ok()
                .filter(|r| {
                    r.mailbox.len() == 32
                        && (1..=MAX_RECEIPT_IDS).contains(&r.ids.len())
                        && r.ids.iter().all(|id| id.len() == 16)
                })
                .map(Self::Receipt)
                .ok_or(Error::Reply("bad receipt")),
            Some((6, attachment)) => AttachmentWire::decode(attachment)
                .ok()
                .and_then(AttachmentWire::open)
                .map(Self::Attachment)
                .ok_or(Error::Reply("bad attachment")),
            Some((7, body)) => match body.first() {
                Some(1 | 6 | 12) => Ok(Self::Forwarded(Box::new(Self::decode(body)?))),
                _ => Err(Error::Reply("bad forward")),
            },
            Some((8, body)) => {
                let (target, operation, hlc, text) =
                    operation_parts(body).ok_or(Error::Reply("bad edit"))?;
                if text.len() > MAX_TEXT_BYTES {
                    return Err(Error::Reply("bad edit"));
                }
                Ok(Self::Edit {
                    target,
                    operation,
                    hlc,
                    text: text.to_vec(),
                })
            }
            Some((9, body)) => {
                let (target, operation, hlc, rest) =
                    operation_parts(body).ok_or(Error::Reply("bad delete"))?;
                if !rest.is_empty() {
                    return Err(Error::Reply("bad delete"));
                }
                Ok(Self::Delete {
                    target,
                    operation,
                    hlc,
                })
            }
            Some((10, body)) => Ok(Self::Presence {
                hlc: u64::from_be_bytes(body.try_into().map_err(|_| Error::Reply("bad presence"))?),
            }),
            Some((11, body)) => Ok(Self::Read {
                up_to: body.try_into().map_err(|_| Error::Reply("bad read"))?,
            }),
            Some((12, body)) => {
                let (id, rest) = body
                    .split_first_chunk::<MESSAGE_ID_BYTES>()
                    .ok_or(Error::Reply("bad timed text"))?;
                let (hlc, text) = rest
                    .split_first_chunk::<8>()
                    .ok_or(Error::Reply("bad timed text"))?;
                if text.len() > MAX_TEXT_BYTES {
                    return Err(Error::Reply("bad timed text"));
                }
                Ok(Self::Text {
                    id: *id,
                    hlc: Some(u64::from_be_bytes(*hlc)),
                    text: text.to_vec(),
                })
            }
            _ => Err(Error::Reply("unknown message kind")),
        }
    }
}

type OperationParts<'a> = ([u8; 16], [u8; 16], u64, &'a [u8]);

fn operation_parts(body: &[u8]) -> Option<OperationParts<'_>> {
    let (header, rest) = body.split_at_checked(OPERATION_HEADER_BYTES)?;
    let (target, tail) = header.split_first_chunk::<MESSAGE_ID_BYTES>()?;
    let (operation, hlc) = tail.split_first_chunk::<MESSAGE_ID_BYTES>()?;
    Some((
        *target,
        *operation,
        u64::from_be_bytes(hlc.try_into().ok()?),
        rest,
    ))
}

/// Leaves out a message's text and sender, so a logged event says nothing about the chat.
impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Joined(group) => f.debug_tuple("Joined").field(group).finish(),
            Self::Message { group, .. } => f
                .debug_struct("Message")
                .field("group", group)
                .finish_non_exhaustive(),
            Self::Interaction { group, .. } => f
                .debug_struct("Interaction")
                .field("group", group)
                .finish_non_exhaustive(),
            Self::Commit(group) => f.debug_tuple("Commit").field(group).finish(),
            Self::Gap(group) => f.debug_tuple("Gap").field(group).finish(),
            Self::UnknownOwner(_) => f.debug_tuple("UnknownOwner").finish_non_exhaustive(),
            Self::Refused(reason) => f.debug_tuple("Refused").field(reason).finish(),
        }
    }
}

/// Publishes `count` single-use KeyPackages of `device` (its MLS state) under the device mailbox
/// key `device_key`, owned by `owner` (hex Peer ID). Seal the state before calling: the private
/// keys are in it.
pub async fn publish_key_packages(
    node: &mut Node,
    device_key: &Identity,
    device: &Device,
    owner: &str,
    count: usize,
) -> Result<(), Error> {
    for _ in 0..count {
        let package = device.key_package(false)?;
        node.publish_key_package(device_key, owner, package, false, 0)
            .await?;
    }
    Ok(())
}

/// Starts a group with every device in `profiles` except `own_mailbox`, other devices of this
/// device's owner included; returns it with the Welcome for each. Fails if any device has no
/// KeyPackage. `profiles` are the newest ones held, this device's owner's among them.
pub async fn invite(
    node: &mut Node,
    device: &Device,
    own_mailbox: &[u8],
    profiles: &[VerifiedProfile],
) -> Result<(Group, Vec<Delivery>), Error> {
    let packages = key_packages(node, own_mailbox, profiles).await?;
    create(device, &packages, profiles)
}

/// The mailbox and KeyPackage of every device in `profiles` except `own_mailbox`.
pub async fn key_packages(
    node: &mut Node,
    own_mailbox: &[u8],
    profiles: &[VerifiedProfile],
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Error> {
    let mut packages: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for owner in profiles {
        for certificate in &owner.profile.devices {
            let address = device_mailbox(certificate)?;
            if address == own_mailbox || packages.iter().any(|(known, _)| *known == address) {
                continue;
            }
            let package = node.fetch_key_package(owner, &address).await?;
            packages.push((address, package));
        }
    }
    Ok(packages)
}

/// Starts a group with `packages` from [`key_packages`]; returns it with the Welcome for each.
pub fn create(
    device: &Device,
    packages: &[(Vec<u8>, Vec<u8>)],
    profiles: &[VerifiedProfile],
) -> Result<(Group, Vec<Delivery>), Error> {
    let keys: Vec<Vec<u8>> = packages
        .iter()
        .map(|(_, package)| package.clone())
        .collect();
    let (group, welcome) = device.create_group(&keys, profiles)?;
    let deliveries = packages
        .iter()
        .map(|(mailbox, _)| Delivery {
            mailbox: mailbox.clone(),
            mls: welcome.clone(),
        })
        .collect();
    Ok((group, deliveries))
}

/// Adds the devices of `packages` from [`key_packages`] to `group`: the commit for its members,
/// the Welcome for the new devices.
pub fn add(
    group: &mut Group,
    device: &Device,
    packages: &[(Vec<u8>, Vec<u8>)],
    profiles: &[VerifiedProfile],
) -> Result<Vec<Delivery>, Error> {
    let keys: Vec<Vec<u8>> = packages
        .iter()
        .map(|(_, package)| package.clone())
        .collect();
    let (commit, welcome) = group.add(device, &keys, profiles)?;
    let mut deliveries = to_members(group, &commit)?;
    deliveries.retain(|delivery| !packages.iter().any(|(new, _)| *new == delivery.mailbox));
    deliveries.extend(packages.iter().map(|(mailbox, _)| Delivery {
        mailbox: mailbox.clone(),
        mls: welcome.clone(),
    }));
    Ok(deliveries)
}

/// Removes the members `profiles` no longer lists from `group`: the commit for the other members.
pub fn remove(
    group: &mut Group,
    device: &Device,
    profiles: &[VerifiedProfile],
) -> Result<Vec<Delivery>, Error> {
    match group.remove_revoked(device, profiles)? {
        Some(commit) => to_members(group, &commit),
        None => Ok(Vec::new()),
    }
}

/// A tag 13 body: kind, HLC, Peer ID, then a flag or a name.
fn peer_change(body: &[u8]) -> Option<Body> {
    let (&kind, rest) = body.split_first()?;
    let (hlc, rest) = rest.split_first_chunk::<8>()?;
    let (peer, value) = rest.split_first_chunk::<PEER_ID_CHARS>()?;
    let flag = match value {
        [0] => Some(false),
        [1] => Some(true),
        _ => None,
    };
    let change = match kind {
        1 => PeerChange::Blocked(flag?),
        2 => PeerChange::Muted(flag?),
        3 => PeerChange::Passed(flag?),
        4 => PeerChange::Name(String::from_utf8(value.to_vec()).ok()?),
        5 => PeerChange::Favorite(flag?),
        6 => PeerChange::Note(String::from_utf8(value.to_vec()).ok()?),
        _ => return None,
    };
    let peer = String::from_utf8(peer.to_vec()).ok()?;
    change.valid(&peer).then_some(Body::PeerChange {
        peer,
        hlc: u64::from_be_bytes(*hlc),
        change,
    })
}

/// Pads and encrypts `body` for every other device in `group`.
pub fn message(group: &mut Group, device: &Device, body: &Body) -> Result<Vec<Delivery>, Error> {
    let floor = match body {
        Body::Interaction(_) | Body::Presence { .. } | Body::Read { .. } => SIGNAL_PADDED_BYTES,
        _ => MIN_PADDED_BYTES,
    };
    let encoded = body.encode();
    Body::decode(&encoded)?;
    let message = group.encrypt(device, &pad_from(&encoded, floor))?;
    to_members(group, &message)
}

/// The deliveries of `commit` to the group's other devices.
pub fn to_members(group: &Group, commit: &[u8]) -> Result<Vec<Delivery>, Error> {
    group
        .peer_device_keys()?
        .into_iter()
        .map(|key| {
            let key: [u8; 32] = key.try_into().map_err(|_| Error::Reply("bad device key"))?;
            Ok(Delivery {
                mailbox: mailbox(&key).to_vec(),
                mls: commit.to_vec(),
            })
        })
        .collect()
}

/// Puts each delivery in its mailbox.
pub async fn send(node: &mut Node, deliveries: &[Delivery]) -> Result<(), Error> {
    for delivery in deliveries {
        node.put(&delivery.mailbox, delivery.mls.clone()).await?;
    }
    Ok(())
}

/// Fetches the mailbox of `device_key` and applies each envelope to `groups`: a Welcome adds a
/// group, a commit or message goes to its group. Returns what happened and the ids to ack after
/// sealing the state. Senders' profiles must be in `profiles`; an envelope for a group not
/// joined yet stays in the mailbox for a later call.
pub async fn receive(
    node: &mut Node,
    device_key: &Identity,
    device: &Device,
    groups: &mut Vec<Group>,
    profiles: &[VerifiedProfile],
) -> Result<(Vec<Event>, Vec<Vec<u8>>), Error> {
    let (envelopes, _more) = node.fetch(device_key).await?;
    Ok(apply_all(device, groups, envelopes, profiles))
}

/// [`receive`] from the union of the mailbox at `first` and at `second`, a node of another
/// replica key: call it on [`Event::Gap`] and once a day. An envelope only `second` holds counts
/// against `first` in `scores` when `first` returned its whole mailbox. Ack the ids at both.
pub async fn cross_check(
    scores: &mut Scores,
    first: &mut Node,
    second: &mut Node,
    device_key: &Identity,
    device: &Device,
    groups: &mut Vec<Group>,
    profiles: &[VerifiedProfile],
) -> Result<(Vec<Event>, Vec<Vec<u8>>), Error> {
    let (mut envelopes, more) = first.fetch(device_key).await?;
    let (others, _) = second.fetch(device_key).await?;
    let held = envelopes.len();
    for envelope in others {
        if !envelopes.iter().any(|known| known.id == envelope.id) {
            envelopes.push(envelope);
        }
    }
    if envelopes.len() > held && !more {
        scores.fault(first.peer(), crate::now());
    }
    // Welcomes first, then by epoch, an epoch's messages before the commit that ends it.
    envelopes.sort_by_key(|envelope| mls::epoch_of(&envelope.ciphertext).ok().flatten());
    Ok(apply_all(device, groups, envelopes, profiles))
}

fn apply_all(
    device: &Device,
    groups: &mut Vec<Group>,
    envelopes: Vec<Envelope>,
    profiles: &[VerifiedProfile],
) -> (Vec<Event>, Vec<Vec<u8>>) {
    let mut events = Vec::new();
    let mut handled = Vec::new();
    for envelope in envelopes {
        let event = match apply(device, groups, &envelope.ciphertext, profiles) {
            Ok(Some(event @ (Event::Gap(_) | Event::UnknownOwner(_)))) => {
                events.push(event);
                continue;
            }
            Ok(Some(event)) => event,
            Ok(None) => continue,
            Err(error) => Event::Refused(error.to_string()),
        };
        events.push(event);
        handled.push(envelope.id);
    }
    (events, handled)
}

pub(crate) fn apply(
    device: &Device,
    groups: &mut Vec<Group>,
    bytes: &[u8],
    profiles: &[VerifiedProfile],
) -> Result<Option<Event>, Error> {
    let Some(id) = mls::group_of(bytes)? else {
        let group = match device.join(bytes, profiles) {
            Err(mls::Error::UnknownOwner(owner) | mls::Error::Revoked(owner)) => {
                return Ok(Some(Event::UnknownOwner(owner)));
            }
            joined => joined?,
        };
        let id = group.id().to_vec();
        groups.push(group);
        return Ok(Some(Event::Joined(id)));
    };
    let Some(group) = groups.iter_mut().find(|group| group.id() == id) else {
        return Ok(None);
    };
    if mls::epoch_of(bytes)?.is_some_and(|(epoch, _)| epoch > group.epoch()) {
        return Ok(Some(Event::Gap(id)));
    }
    let received = match group.process(device, bytes, profiles) {
        Err(mls::Error::UnknownOwner(owner) | mls::Error::Revoked(owner)) => {
            return Ok(Some(Event::UnknownOwner(owner)));
        }
        received => received?,
    };
    Ok(Some(match received {
        Received::Message {
            sender,
            device,
            plaintext,
        } => match Body::decode(unpad(&plaintext).ok_or(Error::Reply("bad padding"))?)? {
            Body::Interaction(event) => Event::Interaction {
                group: id,
                sender,
                device,
                event,
            },
            body => Event::Message {
                group: id,
                sender,
                device,
                body,
            },
        },
        Received::Commit => Event::Commit(id),
    }))
}

pub(crate) fn device_mailbox(certificate: &SignedRecord) -> Result<Vec<u8>, Error> {
    let certificate = DeviceCertificate::decode(certificate.payload.as_slice())
        .map_err(|_| Error::Reply("bad device certificate"))?;
    let key: [u8; 32] = certificate
        .device_key
        .try_into()
        .map_err(|_| Error::Reply("bad device key"))?;
    Ok(mailbox(&key).to_vec())
}

/// `plaintext` after its length, zero-filled to the next power of two from
/// [`MIN_PADDED_BYTES`].
pub fn pad(plaintext: &[u8]) -> Vec<u8> {
    pad_from(plaintext, MIN_PADDED_BYTES)
}

fn pad_from(plaintext: &[u8], floor: usize) -> Vec<u8> {
    let size = (LENGTH_BYTES + plaintext.len())
        .max(floor)
        .next_power_of_two();
    let length = u32::try_from(plaintext.len()).unwrap_or(u32::MAX);
    let mut padded = Vec::with_capacity(size);
    padded.extend_from_slice(&length.to_be_bytes());
    padded.extend_from_slice(plaintext);
    padded.resize(size, 0);
    padded
}

/// The plaintext [`pad`] wrapped, or `None` if the length prefix overruns.
pub fn unpad(padded: &[u8]) -> Option<&[u8]> {
    let (length, rest) = padded.split_first_chunk::<LENGTH_BYTES>()?;
    rest.get(..usize::try_from(u32::from_be_bytes(*length)).ok()?)
}

#[cfg(test)]
mod tests;
