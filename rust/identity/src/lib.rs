//! Long-lived Ed25519 identity key and signed records.
//!
//! The Peer ID is the hex SHA-256 of the identity public key. A record is signed over a domain
//! label and the exact payload bytes that travel on the wire, so a signature for one record kind
//! cannot be replayed as another and no re-encoding is needed before verification.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid public key")]
    InvalidKey,
    #[error("invalid signature")]
    InvalidSignature,
}

/// What a signature covers; each kind of signed data has its own label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Profile,
}

impl Domain {
    fn label(self) -> &'static [u8] {
        match self {
            Domain::Profile => b"dyapp/profile/v1\0",
        }
    }
}

/// A payload with the signer's public key and an Ed25519 signature over `domain || payload`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct SignedRecord {
    #[prost(bytes = "vec", tag = "1")]
    pub public_key: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub signature: Vec<u8>,
}

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
    Sha256::digest(public_key)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn signed_bytes(domain: Domain, payload: &[u8]) -> Vec<u8> {
    [domain.label(), payload].concat()
}

/// The user's identity key. It signs profiles (and later device certificates) and nothing else.
pub struct Identity {
    key: SigningKey,
}

impl Identity {
    pub fn generate() -> Self {
        Self {
            key: SigningKey::generate(&mut rand_core::OsRng),
        }
    }

    pub fn from_secret(secret: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(secret),
        }
    }

    pub fn secret(&self) -> [u8; 32] {
        self.key.to_bytes()
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    pub fn peer_id(&self) -> String {
        peer_id(&self.public_key())
    }

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
        assert_eq!(
            Identity::from_secret(&identity.secret()).peer_id(),
            identity.peer_id()
        );
    }
}
