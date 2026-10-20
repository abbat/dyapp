//! End-to-end encryption with MLS (RFC 9420) through openmls, see ADR 0005.
//!
//! A device signs its MLS messages with its own Ed25519 key. The identity key certifies that key
//! in a [`DeviceCertificate`](dyapp_profile::DeviceCertificate), and the signed certificate is the
//! device's MLS basic credential, so any member checks the chain from a leaf to its owner's Peer
//! ID. Every check also needs the certificate in the owner's newest profile the caller holds, so a
//! device the owner dropped from the list (lost or replaced) can no longer join, commit or send.

use dyapp_identity::{Domain, Identity, SignedRecord};
use dyapp_profile::{DeviceCertificate, VerifiedProfile};
use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use prost::Message as _;

/// The only suite accepted, `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (0x0001).
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
/// How long a new KeyPackage stays valid.
pub const KEY_PACKAGE_LIFETIME_SECONDS: u64 = 30 * 86_400;
// openmls starts a lifetime an hour in the past for clock skew.
const LIFETIME_MARGIN_SECONDS: u64 = 3_600;

/// Why an MLS operation or an incoming MLS message was rejected.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The KeyPackage or Welcome uses a suite other than [`CIPHERSUITE`].
    #[error("unsupported MLS ciphersuite")]
    Ciphersuite,
    /// A member's credential is not a device certificate for its MLS signature key.
    #[error("MLS credential does not chain to an identity")]
    Credential,
    /// The member's certificate is not in its owner's profile among those given.
    #[error("device is not in its owner's current profile")]
    Revoked,
    /// The KeyPackage is valid for longer than [`KEY_PACKAGE_LIFETIME_SECONDS`].
    #[error("KeyPackage lifetime is too long")]
    Lifetime,
    /// The bytes are not the expected kind of MLS message.
    #[error("unexpected MLS message")]
    Unexpected,
    /// openmls rejected the message or failed; the text names the reason, never key bytes.
    #[error("MLS: {0}")]
    Mls(String),
}

fn mls(error: impl std::fmt::Display) -> Error {
    Error::Mls(error.to_string())
}

// The Peer ID behind a device certificate for `signature_key`, and the certificate.
fn certified(
    credential: &Credential,
    signature_key: &[u8],
) -> Result<(String, SignedRecord), Error> {
    let basic = BasicCredential::try_from(credential.clone()).map_err(|_| Error::Credential)?;
    let signed = SignedRecord::decode(basic.identity()).map_err(|_| Error::Credential)?;
    let peer_id = signed
        .verify(Domain::DeviceCertificate)
        .map_err(|_| Error::Credential)?;
    let certificate =
        DeviceCertificate::decode(signed.payload.as_slice()).map_err(|_| Error::Credential)?;
    if certificate.mls_signature_key != signature_key || signature_key == signed.public_key {
        return Err(Error::Credential);
    }
    Ok((peer_id, signed))
}

/// Checks that `credential` is a device certificate signed by an identity key for
/// `signature_key` and listed in that identity's profile in `profiles`, the newest the caller
/// holds; returns the identity's Peer ID.
pub fn verify_member(
    credential: &Credential,
    signature_key: &[u8],
    profiles: &[VerifiedProfile],
) -> Result<String, Error> {
    let (peer_id, certificate) = certified(credential, signature_key)?;
    profiles
        .iter()
        .any(|owner| owner.peer_id == peer_id && owner.profile.devices.contains(&certificate))
        .then_some(peer_id)
        .ok_or(Error::Revoked)
}

fn verify_leaf(leaf: &LeafNode, profiles: &[VerifiedProfile]) -> Result<String, Error> {
    verify_member(leaf.credential(), leaf.signature_key().as_slice(), profiles)
}

/// One device's MLS signature key and the private keys of its KeyPackages and groups.
// ponytail: openmls in-memory storage, so groups and unused KeyPackages die with the process;
// persist them with the client's local store.
pub struct Device {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential: CredentialWithKey,
    certificate: SignedRecord,
}

impl Device {
    /// A device with a new MLS signature key, certified by `identity` together with the device
    /// mailbox key `device_key` at `generation`.
    pub fn new(identity: &Identity, device_key: [u8; 32], generation: u64) -> Result<Self, Error> {
        let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).map_err(mls)?;
        let certificate = identity.sign(
            Domain::DeviceCertificate,
            DeviceCertificate {
                device_key: device_key.to_vec(),
                mls_signature_key: signer.public().to_vec(),
                generation,
            }
            .encode_to_vec(),
        );
        let provider = OpenMlsRustCrypto::default();
        signer.store(provider.storage()).map_err(mls)?;
        let credential = CredentialWithKey {
            credential: BasicCredential::new(certificate.encode_to_vec()).into(),
            signature_key: signer.public().into(),
        };
        Ok(Self {
            provider,
            signer,
            credential,
            certificate,
        })
    }

    /// The signed certificate for the owner's profile device list.
    pub fn certificate(&self) -> &SignedRecord {
        &self.certificate
    }

    /// A TLS-serialized KeyPackage valid for [`KEY_PACKAGE_LIFETIME_SECONDS`]. A single-use one
    /// is gone after its first Welcome; a last-resort one stays.
    pub fn key_package(&self, last_resort: bool) -> Result<Vec<u8>, Error> {
        self.key_package_for(Lifetime::new(KEY_PACKAGE_LIFETIME_SECONDS), last_resort)
    }

    fn key_package_for(&self, lifetime: Lifetime, last_resort: bool) -> Result<Vec<u8>, Error> {
        let capabilities = Capabilities::builder()
            .extensions(vec![ExtensionType::LastResort])
            .build();
        let mut builder = KeyPackage::builder()
            .key_package_lifetime(lifetime)
            .leaf_node_capabilities(capabilities);
        if last_resort {
            builder = builder.mark_as_last_resort();
        }
        builder
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential.clone(),
            )
            .map_err(mls)?
            .key_package()
            .tls_serialize_detached()
            .map_err(mls)
    }

    /// Starts a group with the devices behind `key_packages`; returns it with the Welcome to
    /// send them. Each KeyPackage must be valid, current, at most 30 days long, use
    /// [`CIPHERSUITE`] and carry a credential [`verify_member`] accepts with `profiles`.
    pub fn create_group(
        &self,
        key_packages: &[Vec<u8>],
        profiles: &[VerifiedProfile],
    ) -> Result<(Group, Vec<u8>), Error> {
        let packages = key_packages
            .iter()
            .map(|bytes| self.verify_key_package(bytes, profiles))
            .collect::<Result<Vec<_>, _>>()?;
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .build();
        let mut group = MlsGroup::new(
            &self.provider,
            &self.signer,
            &config,
            self.credential.clone(),
        )
        .map_err(mls)?;
        let (_commit, welcome, _info) = group
            .add_members(&self.provider, &self.signer, &packages)
            .map_err(mls)?;
        group.merge_pending_commit(&self.provider).map_err(mls)?;
        Ok((Group { group }, welcome.to_bytes().map_err(mls)?))
    }

    fn verify_key_package(
        &self,
        bytes: &[u8],
        profiles: &[VerifiedProfile],
    ) -> Result<KeyPackage, Error> {
        let package = KeyPackageIn::tls_deserialize_exact(bytes)
            .map_err(mls)?
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(mls)?;
        if package.ciphersuite() != CIPHERSUITE {
            return Err(Error::Ciphersuite);
        }
        // `validate` has checked that now lies inside the lifetime, so its length bounds it.
        let lifetime = package.life_time();
        if lifetime.not_after().saturating_sub(lifetime.not_before())
            > KEY_PACKAGE_LIFETIME_SECONDS + LIFETIME_MARGIN_SECONDS
        {
            return Err(Error::Lifetime);
        }
        verify_leaf(package.leaf_node(), profiles)?;
        Ok(package)
    }

    /// Joins the group a Welcome invites this device to. The Welcome must use [`CIPHERSUITE`],
    /// target a KeyPackage of this device that is still unused (or last-resort), and list only
    /// members [`verify_member`] accepts with `profiles`.
    pub fn join(&self, welcome: &[u8], profiles: &[VerifiedProfile]) -> Result<Group, Error> {
        let MlsMessageBodyIn::Welcome(welcome) = MlsMessageIn::tls_deserialize_exact(welcome)
            .map_err(mls)?
            .extract()
        else {
            return Err(Error::Unexpected);
        };
        if welcome.ciphersuite() != CIPHERSUITE {
            return Err(Error::Ciphersuite);
        }
        let config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        // Consumes the KeyPackage's private keys: a second Welcome for it finds none.
        let staged =
            StagedWelcome::new_from_welcome(&self.provider, &config, welcome, None).map_err(mls)?;
        for member in staged.members() {
            verify_member(&member.credential, &member.signature_key, profiles)?;
        }
        Ok(Group {
            group: staged.into_group(&self.provider).map_err(mls)?,
        })
    }
}

/// What an incoming group message carried.
#[derive(Debug, PartialEq, Eq)]
pub enum Received {
    /// An application message and the Peer ID of the member who sent it.
    Message {
        /// Peer ID of the sender's identity.
        sender: String,
        /// The decrypted bytes.
        plaintext: Vec<u8>,
    },
    /// A commit, checked and applied: the group is in its next epoch.
    Commit,
}

/// One MLS group: a chat between the devices of its members.
pub struct Group {
    group: MlsGroup,
}

impl Group {
    /// The group ID that names the chat to every member.
    pub fn id(&self) -> &[u8] {
        self.group.group_id().as_slice()
    }

    /// Peer IDs of all members' identities, one per device, in leaf order, revoked ones included.
    pub fn members(&self) -> Result<Vec<String>, Error> {
        self.group
            .members()
            .map(|member| Ok(certified(&member.credential, &member.signature_key)?.0))
            .collect()
    }

    /// Removes every other member [`verify_member`] refuses with `profiles`; returns the commit
    /// to send to the group, or `None` if all members are still listed.
    pub fn remove_revoked(
        &mut self,
        device: &Device,
        profiles: &[VerifiedProfile],
    ) -> Result<Option<Vec<u8>>, Error> {
        let own = self.group.own_leaf_index();
        let revoked: Vec<_> = self
            .group
            .members()
            .filter(|member| {
                member.index != own
                    && verify_member(&member.credential, &member.signature_key, profiles).is_err()
            })
            .map(|member| member.index)
            .collect();
        if revoked.is_empty() {
            return Ok(None);
        }
        let (commit, _, _) = self
            .group
            .remove_members(&device.provider, &device.signer, &revoked)
            .map_err(mls)?;
        self.group
            .merge_pending_commit(&device.provider)
            .map_err(mls)?;
        Ok(Some(commit.to_bytes().map_err(mls)?))
    }

    /// Encrypts and signs `plaintext` for the current epoch; returns the MLS message bytes.
    pub fn encrypt(&mut self, device: &Device, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        self.group
            .create_message(&device.provider, &device.signer, plaintext)
            .map_err(mls)?
            .to_bytes()
            .map_err(mls)
    }

    /// Decrypts an application message or applies a commit; the sender and every new or updated
    /// leaf must pass [`verify_member`] with `profiles`. A refused commit, or one that changes the
    /// group's extensions, leaves the group as it was.
    pub fn process(
        &mut self,
        device: &Device,
        message: &[u8],
        profiles: &[VerifiedProfile],
    ) -> Result<Received, Error> {
        let message = MlsMessageIn::tls_deserialize_exact(message)
            .map_err(mls)?
            .try_into_protocol_message()
            .map_err(|_| Error::Unexpected)?;
        let processed = self
            .group
            .process_message(&device.provider, message)
            .map_err(mls)?;
        let sender = processed.sender().clone();
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => {
                let Sender::Member(index) = sender else {
                    return Err(Error::Unexpected);
                };
                let member = self.group.member_at(index).ok_or(Error::Credential)?;
                Ok(Received::Message {
                    sender: verify_member(&member.credential, &member.signature_key, profiles)?,
                    plaintext: message.into_bytes(),
                })
            }
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                let changes_extensions = commit
                    .queued_proposals()
                    .any(|queued| matches!(queued.proposal(), Proposal::GroupContextExtensions(_)));
                let leaf = |leaf: &LeafNode| verify_leaf(leaf, profiles);
                let refused = commit
                    .add_proposals()
                    .map(|add| leaf(add.add_proposal().key_package().leaf_node()))
                    .chain(
                        commit
                            .update_proposals()
                            .map(|update| leaf(update.update_proposal().leaf_node())),
                    )
                    .chain(commit.update_path_leaf_node().map(leaf))
                    .find_map(Result::err);
                if changes_extensions {
                    return Err(Error::Credential);
                }
                if let Some(error) = refused {
                    return Err(error);
                }
                self.group
                    .merge_staged_commit(&device.provider, *commit)
                    .map_err(mls)?;
                Ok(Received::Commit)
            }
            _ => Err(Error::Unexpected),
        }
    }

    /// A secret of `length` bytes from the current epoch's exporter, as for per-call video keys;
    /// every member derives the same bytes for the same `label` and `context`.
    pub fn export_secret(
        &self,
        device: &Device,
        label: &str,
        context: &[u8],
        length: usize,
    ) -> Result<Vec<u8>, Error> {
        self.group
            .export_secret(device.provider.crypto(), label, context, length)
            .map_err(mls)
    }
}

#[cfg(test)]
mod tests;
