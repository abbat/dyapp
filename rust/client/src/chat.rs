//! Chats as MLS groups delivered through device mailboxes. Every Welcome, commit and message is
//! MLS bytes in an envelope, one envelope per recipient device, so a node stores only
//! ciphertext. A message's plaintext is padded to the next power of two from 1 KiB before
//! encryption, so its size tells little about its content; a like, unlike, view or presence pads
//! from 256 bytes so its envelope fits the node's 1 KiB signal limit.
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
use dyapp_profile::{DeviceCertificate, VerifiedProfile};
use prost::Message as _;

/// The smallest padded plaintext.
pub const MIN_PADDED_BYTES: usize = 1024;
/// The smallest padded interaction plaintext.
pub const SIGNAL_PADDED_BYTES: usize = 256;
/// Largest signed payload of a signal envelope the node accepts.
pub const MAX_SIGNAL_BYTES: usize = 1024;
/// Maximum text bytes in a chat text or edit, before MLS padding.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
const MESSAGE_ID_BYTES: usize = 16;
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

/// What an application message carries: the first plaintext byte names the kind, so a chat text
/// is never read as an interaction.
#[derive(Clone, PartialEq, Eq)]
pub enum Body {
    /// User text with its logical ID, tag 1.
    Text {
        /// Stable logical ID shared across recipient devices.
        id: [u8; 16],
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
    /// An encrypted blob on `/dyapp/media` and what opens it, tag 6.
    Attachment(Attachment),
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
}

// The wire form of an `Attachment`; previews and media types may take later tags.
#[derive(Clone, PartialEq, Eq, prost::Message)]
struct AttachmentWire {
    #[prost(bytes = "vec", tag = "1")]
    key: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    plaintext_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    blob_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    release_secret: Vec<u8>,
    #[prost(uint64, tag = "5")]
    size: u64,
    #[prost(bytes = "vec", tag = "6")]
    preview: Vec<u8>,
    #[prost(string, tag = "7")]
    mime: String,
    #[prost(uint32, tag = "8")]
    width: u32,
    #[prost(uint32, tag = "9")]
    height: u32,
}

impl AttachmentWire {
    fn open(self) -> Option<Attachment> {
        let attachment = Attachment {
            key: self.key.try_into().ok()?,
            plaintext_sha256: self.plaintext_sha256.try_into().ok()?,
            blob_sha256: self.blob_sha256.try_into().ok()?,
            release_secret: self.release_secret.try_into().ok()?,
            size: self.size,
            preview: self.preview,
            mime: self.mime,
            width: self.width,
            height: self.height,
        };
        attachment.check().ok()?;
        Some(attachment)
    }
}

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
    /// Makes a text body with a caller-owned logical ID, shared by all device deliveries.
    pub fn text_with_id(id: [u8; 16], text: Vec<u8>) -> Self {
        Self::Text { id, text }
    }

    /// The tag byte, then the bounded body bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Text { id, text } => [&[1], id.as_slice(), text.as_slice()].concat(),
            Self::Interaction(event) => [&[2], event.encode().as_slice()].concat(),
            Self::TokenRequest(request) => [&[3], request.encode_to_vec().as_slice()].concat(),
            Self::TokenAnswer(answer) => [&[4], answer.encode_to_vec().as_slice()].concat(),
            Self::Receipt(receipt) => [&[5], receipt.encode_to_vec().as_slice()].concat(),
            Self::Attachment(attachment) => {
                let wire = AttachmentWire {
                    key: attachment.key.to_vec(),
                    plaintext_sha256: attachment.plaintext_sha256.to_vec(),
                    blob_sha256: attachment.blob_sha256.to_vec(),
                    release_secret: attachment.release_secret.to_vec(),
                    size: attachment.size,
                    preview: attachment.preview.clone(),
                    mime: attachment.mime.clone(),
                    width: attachment.width,
                    height: attachment.height,
                };
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
                    text: text.to_vec(),
                })
            }
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
                Some(1 | 6) => Ok(Self::Forwarded(Box::new(Self::decode(body)?))),
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
    let mut mailboxes = Vec::new();
    let mut packages = Vec::new();
    for owner in profiles {
        for certificate in &owner.profile.devices {
            let address = device_mailbox(certificate)?;
            if address == own_mailbox || mailboxes.contains(&address) {
                continue;
            }
            packages.push(node.fetch_key_package(owner, &address).await?);
            mailboxes.push(address);
        }
    }
    let (group, welcome) = device.create_group(&packages, profiles)?;
    let deliveries = mailboxes
        .into_iter()
        .map(|mailbox| Delivery {
            mailbox,
            mls: welcome.clone(),
        })
        .collect();
    Ok((group, deliveries))
}

/// Pads and encrypts `body` for every other device in `group`.
pub fn message(group: &mut Group, device: &Device, body: &Body) -> Result<Vec<Delivery>, Error> {
    let floor = match body {
        Body::Interaction(_) | Body::Presence { .. } => SIGNAL_PADDED_BYTES,
        _ => MIN_PADDED_BYTES,
    };
    let encoded = body.encode();
    Body::decode(&encoded)?;
    let message = group.encrypt(device, &pad_from(&encoded, floor))?;
    to_members(group, &message)
}

/// The deliveries of `commit`, e.g. from [`Group::remove_revoked`], to the group's other devices.
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

/// Whether the owner's newest profile `owner` no longer lists `device`: the device was revoked
/// and must wipe its keys and MLS state.
pub fn revoked(device: &Device, owner: &VerifiedProfile) -> bool {
    !owner.profile.devices.contains(device.certificate())
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
            Err(mls::Error::UnknownOwner(owner)) => return Ok(Some(Event::UnknownOwner(owner))),
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
    Ok(Some(match group.process(device, bytes, profiles)? {
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

fn device_mailbox(certificate: &SignedRecord) -> Result<Vec<u8>, Error> {
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
