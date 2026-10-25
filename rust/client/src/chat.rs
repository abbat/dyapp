//! Chats as MLS groups delivered through device mailboxes. Every Welcome, commit and message is
//! MLS bytes in an envelope, one envelope per recipient device, so a node stores only
//! ciphertext. A message's plaintext is padded to the next power of two from 1 KiB before
//! encryption, so its size tells little about its content; a like, unlike or view pads from
//! 256 bytes so its envelope fits the node's 1 KiB signal limit.
//!
//! Calls that change MLS state return the [`Delivery`] list instead of sending it: seal and
//! store the device state ([`Device::seal`]) first, then [`send`] it. Likewise [`receive`]
//! returns the envelope ids to [`Node::ack`] once the new state is stored.

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
    /// User text, tag 1.
    Text(Vec<u8>),
    /// A like, unlike or view, tag 2.
    Interaction(Interaction),
    /// A contact device asks the owner for WriteTokens, tag 3.
    TokenRequest(TokenRequest),
    /// The owner's signed WriteTokens, tag 4.
    TokenAnswer(TokenAnswer),
    /// A device received writes in its mailbox, tag 5.
    Receipt(Receipt),
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
    /// The tag byte, then the text or the encoded interaction.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Text(text) => [&[1], text.as_slice()].concat(),
            Self::Interaction(event) => [&[2], event.encode().as_slice()].concat(),
            Self::TokenRequest(request) => [&[3], request.encode_to_vec().as_slice()].concat(),
            Self::TokenAnswer(answer) => [&[4], answer.encode_to_vec().as_slice()].concat(),
            Self::Receipt(receipt) => [&[5], receipt.encode_to_vec().as_slice()].concat(),
        }
    }

    /// Refuses an unknown or missing tag, a malformed interaction and a token body that is empty,
    /// too long or holds a field of the wrong length.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        match bytes.split_first() {
            Some((1, text)) => Ok(Self::Text(text.to_vec())),
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
            _ => Err(Error::Reply("unknown message kind")),
        }
    }
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
        Body::Interaction(_) => SIGNAL_PADDED_BYTES,
        _ => MIN_PADDED_BYTES,
    };
    let message = group.encrypt(device, &pad_from(&body.encode(), floor))?;
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
mod tests {
    use super::{
        apply_all, message, pad, unpad, Body, Delivery, Event, Receipt, TokenAnswer, TokenAsk,
        TokenRequest, MAX_RECEIPT_IDS, MAX_TOKENS, MIN_PADDED_BYTES,
    };
    use crate::interactions::{Interaction, Kind};
    use crate::Envelope;
    use dyapp_identity::{Domain, Identity};
    use dyapp_messaging::mls::Device;
    use dyapp_p2p_net::proto;
    use dyapp_profile::{Profile, VerifiedProfile};
    use prost::Message as _;

    fn person() -> (Identity, Device, VerifiedProfile) {
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

    fn envelope(id: u8, ciphertext: Vec<u8>) -> Envelope {
        Envelope {
            id: vec![id],
            sender_key: Vec::new(),
            ciphertext,
        }
    }

    #[test]
    fn applies_a_welcome_then_a_message_and_acks_both() {
        let (alice_identity, alice, alice_profile) = person();
        let (_, bob, bob_profile) = person();
        let profiles = [alice_profile, bob_profile];
        let package = bob.key_package(false).unwrap();
        let (mut group, welcome) = alice.create_group(&[package], &profiles).unwrap();
        let mut groups = Vec::new();
        let (events, acked) = apply_all(&bob, &mut groups, vec![envelope(1, welcome)], &profiles);
        assert!(events == [Event::Joined(group.id().to_vec())]);
        assert_eq!(acked, [vec![1]]);
        let sent = message(&mut group, &alice, &Body::Text(b"hi".to_vec())).unwrap();
        let (events, acked) = apply_all(
            &bob,
            &mut groups,
            vec![envelope(2, sent[0].mls.clone())],
            &profiles,
        );
        assert!(
            events
                == [Event::Message {
                    group: group.id().to_vec(),
                    sender: alice_identity.peer_id(),
                    body: Body::Text(b"hi".to_vec()),
                }]
        );
        assert_eq!(acked, [vec![2]]);

        let like = Interaction {
            kind: Kind::Like,
            event_id: [7; 16],
            hlc: 42,
        };
        let sent = message(&mut group, &alice, &Body::Interaction(like)).unwrap();
        // Padded from 256 bytes, not 1 KiB.
        assert!(
            sent[0].mls.len() < MIN_PADDED_BYTES,
            "{}",
            sent[0].mls.len()
        );
        let (events, acked) = apply_all(
            &bob,
            &mut groups,
            vec![envelope(3, sent[0].mls.clone())],
            &profiles,
        );
        let [Event::Interaction { device, event, .. }] = events.as_slice() else {
            panic!("expected decrypted interaction");
        };
        assert!(*event == like);
        assert_eq!(*device, alice.device_key().unwrap());
        assert_eq!(acked, [vec![3]]);

        // An untagged plaintext, as before the frame, is refused and still acked.
        let raw = group.encrypt(&alice, &pad(b"hi")).unwrap();
        let (events, acked) = apply_all(&bob, &mut groups, vec![envelope(4, raw)], &profiles);
        assert!(events == [Event::Refused("bad reply: unknown message kind".into())]);
        assert_eq!(acked, [vec![4]]);
    }

    #[test]
    fn body_frame_is_strict() {
        let like = Interaction {
            kind: Kind::Like,
            event_id: [7; 16],
            hlc: 42,
        };
        for body in [
            Body::Text(Vec::new()),
            Body::Text(b"hi".to_vec()),
            Body::Interaction(like),
        ] {
            assert!(Body::decode(&body.encode()).ok() == Some(body));
        }
        let tagged = Body::Interaction(like).encode();
        for bad in [
            &b""[..],
            &[0],
            &[5],
            &[3],
            &[4],
            &[2, 1],
            &tagged[..tagged.len() - 1],
        ] {
            assert!(Body::decode(bad).is_err(), "{bad:?}");
        }
        // A text that starts like an interaction stays text.
        let text = [&[1], &tagged[..]].concat();
        assert!(Body::decode(&text).ok() == Some(Body::Text(tagged)));
    }

    #[test]
    fn token_bodies_are_strict() {
        let ask = TokenAsk {
            mailbox: vec![1; 32],
            epoch: 20_000,
            token_key: vec![2; 32],
        };
        let request = |asks: Vec<TokenAsk>| Body::TokenRequest(TokenRequest { asks });
        let answer = |signed_tokens: Vec<Vec<u8>>| Body::TokenAnswer(TokenAnswer { signed_tokens });
        for body in [
            request(vec![ask.clone()]),
            request(vec![ask.clone(); MAX_TOKENS]),
            answer(vec![vec![9; 200]]),
            answer(vec![vec![9]; MAX_TOKENS]),
        ] {
            assert!(Body::decode(&body.encode()).ok() == Some(body));
        }
        let short = |mailbox: usize, token_key: usize| TokenAsk {
            mailbox: vec![1; mailbox],
            token_key: vec![2; token_key],
            ..ask.clone()
        };
        for bad in [
            request(Vec::new()),
            request(vec![ask.clone(); MAX_TOKENS + 1]),
            request(vec![ask.clone(), short(31, 32)]),
            request(vec![short(32, 33)]),
            answer(Vec::new()),
            answer(vec![vec![9]; MAX_TOKENS + 1]),
            answer(vec![vec![9], Vec::new()]),
        ] {
            assert!(Body::decode(&bad.encode()).is_err());
        }
        assert!(Body::decode(&[3, 0xff]).is_err());
    }

    #[test]
    fn receipts_are_strict() {
        let receipt = |mailbox: usize, ids: Vec<Vec<u8>>| {
            Body::Receipt(Receipt {
                mailbox: vec![1; mailbox],
                ids,
            })
        };
        for body in [
            receipt(32, vec![vec![2; 16]]),
            receipt(32, vec![vec![2; 16]; MAX_RECEIPT_IDS]),
        ] {
            assert!(Body::decode(&body.encode()).ok() == Some(body));
        }
        for bad in [
            receipt(31, vec![vec![2; 16]]),
            receipt(32, Vec::new()),
            receipt(32, vec![vec![2; 16]; MAX_RECEIPT_IDS + 1]),
            receipt(32, vec![vec![2; 16], vec![2; 15]]),
        ] {
            assert!(Body::decode(&bad.encode()).is_err());
        }
    }

    #[test]
    fn debug_shows_no_text_sender_or_mailbox() {
        let event = Event::Message {
            group: vec![1],
            sender: "peer-of-bob".into(),
            body: Body::Text(b"secret text".to_vec()),
        };
        assert_eq!(format!("{event:?}"), "Message { group: [1], .. }");
        let delivery = Delivery {
            mailbox: vec![0xab; 32],
            mls: vec![0; 3],
        };
        assert_eq!(format!("{delivery:?}"), "Delivery { mls_len: 3, .. }");
    }

    #[test]
    fn padding_hides_length_up_to_a_power_of_two() {
        for (length, size) in [
            (0, 1024),
            (200, 1024),
            (1020, 1024),
            (1021, 2048),
            (1500, 2048),
            (5000, 8192),
        ] {
            let plaintext = vec![7u8; length];
            let padded = pad(&plaintext);
            assert_eq!(padded.len(), size, "{length}");
            assert_eq!(unpad(&padded), Some(plaintext.as_slice()));
        }
        assert_eq!(unpad(&[0, 0, 4, 1, 0]), None);
        assert_eq!(unpad(&[0, 0]), None);
        assert!(pad(b"").len() == MIN_PADDED_BYTES);
    }

    #[test]
    fn short_signal_padding_fits_the_signed_payload_limit() {
        let (alice_id, alice, alice_profile) = person();
        let (bob_id, bob, bob_profile) = person();
        let profiles = [alice_profile, bob_profile];
        let package = bob.key_package(false).unwrap();
        let (mut group, _) = alice.create_group(&[package], &profiles).unwrap();
        let token = bob_id.sign(
            Domain::WriteToken,
            proto::WriteToken {
                mailbox: vec![0; 32],
                token_key: alice_id.public_key().to_vec(),
                epoch: u32::MAX,
            }
            .encode_to_vec(),
        );
        let payload_size =
            |group: &mut dyapp_messaging::mls::Group, plaintext_len: usize, floor| {
                let size = (4 + plaintext_len).max(floor).next_power_of_two();
                let mut padded = vec![0; size];
                padded[..4].copy_from_slice(&(plaintext_len as u32).to_be_bytes());
                let ciphertext = group.encrypt(&alice, &padded).unwrap();
                proto::Envelope {
                    id: vec![0; 16],
                    mailbox: vec![0; 32],
                    ciphertext,
                    class: proto::EnvelopeClass::Signal.into(),
                    write_token: Some(token.clone()),
                }
                .encoded_len()
            };
        let like = payload_size(&mut group, 16, 256);
        // 508 bytes plus the length prefix fills the 512-byte padding bucket.
        let introduction = payload_size(&mut group, 508, 256);
        let ordinary_padding = payload_size(&mut group, 16, MIN_PADDED_BYTES);
        assert!(like <= 1024, "like signed payload: {like}");
        assert!(
            introduction <= 1024,
            "introduction signed payload: {introduction}"
        );
        assert!(
            ordinary_padding > 1024,
            "ordinary padding: {ordinary_padding}"
        );
    }
}
