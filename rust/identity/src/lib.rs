//! Long-lived Ed25519 identity key and signed records.
//!
//! The Peer ID is the hex SHA-256 of the identity public key. A record is signed over a domain
//! label and the exact payload bytes that travel on the wire, so a signature for one record kind
//! cannot be replayed as another and no re-encoding is needed before verification.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Why a signed record or key was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The public key is not a valid 32-byte Ed25519 key.
    #[error("invalid public key")]
    InvalidKey,
    /// The signature is malformed or does not match the key, domain and payload.
    #[error("invalid signature")]
    InvalidSignature,
}

// One registry generates variants, labels and the complete test set together.
macro_rules! signature_domains {
    ($($(#[$doc:meta])* $variant:ident => $label:literal),+ $(,)?) => {
        /// What a signature covers; each kind of signed data has its own label.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Domain {
            $($(#[$doc])* $variant),+
        }

        impl Domain {
            fn label(self) -> &'static [u8] {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            #[cfg(test)]
            const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
    };
}

signature_domains! {
    /// A public profile, signed by the owner's identity key.
    Profile => b"dyapp/profile/v1\0",
    /// A mailbox envelope, signed by the sender's device key.
    Envelope => b"dyapp/envelope/v1\0",
    /// A mailbox fetch, signed by the mailbox's device key.
    MailboxFetch => b"dyapp/mailbox-fetch/v1\0",
    /// A mailbox ack, signed by the mailbox's device key.
    MailboxAck => b"dyapp/mailbox-ack/v1\0",
    /// The list of an owner's media blobs, signed by the owner's identity key.
    MediaKeep => b"dyapp/media-keep/v1\0",
    /// Chat attachment blobs, signed by the sender's identity key.
    MediaAttach => b"dyapp/media-attach/v1\0",
    /// A liveness signal that keeps the owner's profile stored, signed by the identity key.
    Heartbeat => b"dyapp/heartbeat/v1\0",
    /// An operator's deny list, signed by the node's libp2p key.
    DenyList => b"dyapp/deny-list/v1\0",
}

mod generated {
    #![allow(clippy::pedantic)]
    // prost emits no docs for messages without proto comments.
    #![allow(missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.identity.rs"));
}
pub use generated::SignedRecord;

impl SignedRecord {
    /// Checks the signature and returns the signer's Peer ID.
    pub fn verify(&self, domain: Domain) -> Result<String, Error> {
        let key: [u8; 32] = self
            .public_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::InvalidKey)?;
        let key = VerifyingKey::from_bytes(&key).map_err(|_| Error::InvalidKey)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| Error::InvalidSignature)?;
        key.verify_strict(&signed_bytes(domain, &self.payload), &signature)
            .map_err(|_| Error::InvalidSignature)?;
        Ok(peer_id(key.as_bytes()))
    }
}

/// Hex SHA-256 of an identity public key.
pub fn peer_id(public_key: &[u8; 32]) -> String {
    key_hash(public_key)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// SHA-256 of a public key: the raw peer ID of an identity, the mailbox address of a device.
pub fn key_hash(public_key: &[u8; 32]) -> [u8; 32] {
    sha256(public_key)
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn signed_bytes(domain: Domain, payload: &[u8]) -> Vec<u8> {
    [domain.label(), payload].concat()
}

/// An Ed25519 signing key: the user's identity key, which signs profiles (and later device
/// certificates) and nothing else, or a device key, which signs mailbox requests.
pub struct Identity {
    key: SigningKey,
}

impl Identity {
    /// A new key from the OS random number generator; panics if it is unavailable.
    pub fn generate() -> Self {
        let mut secret = Zeroizing::new([0u8; 32]);
        // Like OsRng before it: no key without the OS random number generator.
        getrandom::fill(&mut secret[..]).expect("OS random number generator");
        Self::from_secret(&secret)
    }

    /// The key whose Ed25519 secret is `secret`.
    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(secret),
        }
    }

    /// Owned secret bytes, cleared when this guard is dropped.
    pub fn secret(&self) -> Zeroizing<[u8; 32]> {
        let mut secret = Zeroizing::new([0u8; 32]);
        secret.copy_from_slice(self.key.as_bytes());
        secret
    }

    /// The Ed25519 public key bytes.
    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// Hex SHA-256 of the public key: see [`peer_id`].
    pub fn peer_id(&self) -> String {
        peer_id(&self.public_key())
    }

    /// Signs `payload` in `domain` and returns it with the public key and signature.
    pub fn sign(&self, domain: Domain, payload: Vec<u8>) -> SignedRecord {
        let signature = self.key.sign(&signed_bytes(domain, &payload));
        SignedRecord {
            public_key: self.public_key().to_vec(),
            payload,
            signature: signature.to_bytes().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_domain_has_a_unique_prefix_free_label_separate_from_libp2p() {
        // Verified against the locked primary crate sources:
        // libp2p-core 0.44.0 peer_record.rs (legacy and standard domains);
        // libp2p-noise 0.47.0 protocol.rs, libp2p-tls 0.7.0 certificate.rs,
        // libp2p-gossipsub 0.50.0 protocol.rs. Recheck when these dependencies change.
        let libp2p_labels: &[&[u8]] = &[
            b"libp2p-routing-state",
            b"libp2p-peer-record",
            b"noise-libp2p-static-key:",
            b"libp2p-tls-handshake:",
            b"libp2p-pubsub:",
        ];
        for domain in Domain::ALL {
            let label = domain.label();
            assert!(label.starts_with(b"dyapp/"), "{domain:?}");
            assert_eq!(label.last(), Some(&0), "{domain:?}");
            assert!(!label[..label.len() - 1].contains(&0), "{domain:?}");
            for other in Domain::ALL.iter().filter(|other| *other != domain) {
                assert!(!label.starts_with(other.label()), "{domain:?} vs {other:?}");
            }
            for external in libp2p_labels {
                assert!(
                    !label.starts_with(external) && !external.starts_with(label),
                    "{domain:?} overlaps libp2p label"
                );
            }
        }
    }

    #[test]
    fn signatures_verify_only_in_their_own_domain() {
        let identity = Identity::generate();
        for domain in Domain::ALL {
            let record = identity.sign(*domain, b"same payload".to_vec());
            for other in Domain::ALL {
                let result = record.verify(*other);
                if domain == other {
                    assert_eq!(result, Ok(identity.peer_id()), "{domain:?}");
                } else {
                    assert_eq!(
                        result,
                        Err(Error::InvalidSignature),
                        "{domain:?} vs {other:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sign_verify_and_tamper() {
        let identity = Identity::generate();
        let record = identity.sign(Domain::Profile, b"hello".to_vec());
        assert_eq!(record.verify(Domain::Profile), Ok(identity.peer_id()));
        assert_eq!(identity.peer_id().len(), 64);

        let mut tampered = record.clone();
        tampered.payload[0] ^= 1;
        assert_eq!(
            tampered.verify(Domain::Profile),
            Err(Error::InvalidSignature)
        );

        let mut other_key = record.clone();
        other_key.public_key = Identity::generate().public_key().to_vec();
        assert_eq!(
            other_key.verify(Domain::Profile),
            Err(Error::InvalidSignature)
        );

        let mut short = record;
        short.public_key.pop();
        assert_eq!(short.verify(Domain::Profile), Err(Error::InvalidKey));
    }

    #[test]
    fn secret_round_trip_keeps_peer_id() {
        let identity = Identity::generate();
        let secret: Zeroizing<[u8; 32]> = identity.secret();
        assert_eq!(Identity::from_secret(&secret).peer_id(), identity.peer_id());
    }
}
