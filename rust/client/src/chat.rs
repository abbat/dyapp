//! Chats as MLS groups delivered through device mailboxes. Every Welcome, commit and message is
//! MLS bytes in an envelope, one envelope per recipient device, so a node stores only
//! ciphertext. A message's plaintext is padded to the next power of two from 1 KiB before
//! encryption, so its size tells little about its content.
//!
//! Calls that change MLS state return the [`Delivery`] list instead of sending it: seal and
//! store the device state ([`Device::seal`]) first, then [`send`] it. Likewise [`receive`]
//! returns the envelope ids to [`Node::ack`] once the new state is stored.

use crate::scores::Scores;
use crate::{mailbox, Envelope, Error, Node};
use dyapp_identity::{Identity, SignedRecord};
use dyapp_messaging::mls::{self, Device, Group, Received};
use dyapp_profile::{DeviceCertificate, VerifiedProfile};
use prost::Message as _;

/// The smallest padded plaintext.
pub const MIN_PADDED_BYTES: usize = 1024;
// The big-endian plaintext length that starts a padded plaintext.
const LENGTH_BYTES: usize = 4;

/// MLS bytes for one device mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// The recipient device's mailbox address.
    pub mailbox: Vec<u8>,
    /// A Welcome, commit or application message.
    pub mls: Vec<u8>,
}

/// What a fetched envelope carried.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// This device joined the group with this ID.
    Joined(Vec<u8>),
    /// A member's message.
    Message {
        /// The group ID.
        group: Vec<u8>,
        /// Peer ID of the sender's identity.
        sender: String,
        /// The unpadded text.
        plaintext: Vec<u8>,
    },
    /// A commit moved the group with this ID to its next epoch.
    Commit(Vec<u8>),
    /// A message from a later epoch of the group with this ID: a commit is missing, maybe hidden
    /// by the node. The envelope stays in the mailbox; run [`cross_check`].
    Gap(Vec<u8>),
    /// The envelope was refused and acked: no group took it.
    Refused(String),
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

/// Pads and encrypts `plaintext` for every other device in `group`.
pub fn message(
    group: &mut Group,
    device: &Device,
    plaintext: &[u8],
) -> Result<Vec<Delivery>, Error> {
    let message = group.encrypt(device, &pad(plaintext))?;
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
            Ok(Some(Event::Gap(group))) => {
                events.push(Event::Gap(group));
                continue;
            }
            Ok(Some(event)) => event,
            Ok(None) => continue,
            // ponytail: a refused envelope is dropped; keep it for a retry once a profile can
            // be refreshed and a Welcome rechecked without consuming its KeyPackage.
            Err(error) => Event::Refused(error.to_string()),
        };
        events.push(event);
        handled.push(envelope.id);
    }
    (events, handled)
}

fn apply(
    device: &Device,
    groups: &mut Vec<Group>,
    bytes: &[u8],
    profiles: &[VerifiedProfile],
) -> Result<Option<Event>, Error> {
    let Some(id) = mls::group_of(bytes)? else {
        let group = device.join(bytes, profiles)?;
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
        Received::Message { sender, plaintext } => Event::Message {
            group: id,
            sender,
            plaintext: unpad(&plaintext)
                .ok_or(Error::Reply("bad padding"))?
                .to_vec(),
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
    let size = (LENGTH_BYTES + plaintext.len())
        .max(MIN_PADDED_BYTES)
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
    use super::{pad, unpad, MIN_PADDED_BYTES};

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
}
