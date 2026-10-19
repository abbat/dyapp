//! Public profile: a plaintext document signed by its owner's identity key.
//!
//! Each published version carries a version number; the highest validly signed version wins.
//! Deleting a profile publishes a tombstone: a signed version with `deleted` set and every other
//! field but the proof of work empty. Nodes keep the tombstone so older versions are not
//! re-imported.

use argon2::{Algorithm, Argon2, Params, Version};
use dyapp_identity::{Domain, Identity, SignedRecord};
use prost::Message;

/// Largest accepted profile payload. Media are not part of it: the profile only links to blobs.
pub const MAX_PAYLOAD_LEN: usize = 1024 * 1024;

/// Why a profile record was rejected.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The record signature or key is invalid.
    #[error(transparent)]
    Signature(#[from] dyapp_identity::Error),
    /// The payload is not a protobuf `Profile`.
    #[error("malformed profile")]
    Decode,
    /// The profile breaks a content rule; the text names it.
    #[error("invalid profile: {0}")]
    Invalid(&'static str),
    /// The version is not above the stored one.
    #[error("profile version is not newer than the stored one")]
    Stale,
}

/// Longest accepted `place`, in Unicode code points.
pub const MAX_PLACE_CHARS: usize = 1024;
/// Most photos a profile links.
pub const MAX_PHOTOS: usize = 16;
/// Most blobs one full photo is split into.
pub const MAX_PHOTO_BLOBS: usize = 64;

mod generated {
    #![allow(clippy::pedantic)]
    // prost emits no docs for messages without proto comments.
    #![allow(missing_docs)]
    include!(concat!(env!("OUT_DIR"), "/dyapp.profile.rs"));
}
pub use generated::{Photo, Profile};

impl Profile {
    /// A deletion marker: version and proof of work set, every other field empty.
    pub fn tombstone(version: u64, pow_nonce: u64, pow_bits: u32) -> Self {
        Self {
            version,
            deleted: true,
            pow_nonce,
            pow_bits,
            ..Self::default()
        }
    }

    /// Signs the encoded profile with the owner's identity key.
    pub fn sign(&self, identity: &Identity) -> SignedRecord {
        identity.sign(Domain::Profile, self.encode_to_vec())
    }

    fn validate(&self) -> Result<(), Error> {
        if self.version == 0 {
            return Err(Error::Invalid("version must start at 1"));
        }
        if self.deleted && *self != Self::tombstone(self.version, self.pow_nonce, self.pow_bits) {
            return Err(Error::Invalid("tombstone carries data"));
        }
        if !self.country.is_empty()
            && !(self.country.len() == 2 && self.country.bytes().all(|b| b.is_ascii_uppercase()))
        {
            return Err(Error::Invalid("country is not an ISO 3166-1 alpha-2 code"));
        }
        if let (Some(from), Some(to)) = (self.income_from, self.income_to) {
            if from > to {
                return Err(Error::Invalid("income range is reversed"));
            }
        }
        let place = &self.place;
        if place.chars().count() > MAX_PLACE_CHARS || place.chars().any(char::is_control) {
            return Err(Error::Invalid("place is too long or not printable"));
        }
        let hash = |h: &Vec<u8>| h.len() == 32;
        if self.photos.len() > MAX_PHOTOS
            || !self.photos.iter().all(|p| {
                hash(&p.thumbnail)
                    && !p.full.is_empty()
                    && p.full.len() <= MAX_PHOTO_BLOBS
                    && p.full.iter().all(hash)
            })
        {
            return Err(Error::Invalid("photos are not blob hashes or too many"));
        }
        Ok(())
    }
}

/// A profile whose signature and content have been checked.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedProfile {
    /// Signer's Peer ID.
    pub peer_id: String,
    /// The decoded, validated profile.
    pub profile: Profile,
}

/// Checks size, signature, encoding and content rules of a profile record.
pub fn verify(record: &SignedRecord) -> Result<VerifiedProfile, Error> {
    if record.payload.len() > MAX_PAYLOAD_LEN {
        return Err(Error::Invalid("profile too large"));
    }
    let peer_id = record.verify(Domain::Profile)?;
    let profile = Profile::decode(record.payload.as_slice()).map_err(|_| Error::Decode)?;
    profile.validate()?;
    Ok(VerifiedProfile { peer_id, profile })
}

/// Verifies `record` and accepts it only if it is newer than `current`, the owner's stored
/// version (already verified when it was stored).
pub fn accept(record: &SignedRecord, current: Option<u64>) -> Result<VerifiedProfile, Error> {
    let verified = verify(record)?;
    verified.check_newer(current)?;
    Ok(verified)
}

impl VerifiedProfile {
    /// Fails with [`Error::Stale`] unless this version is above `current`, the stored one.
    pub fn check_newer(&self, current: Option<u64>) -> Result<(), Error> {
        match current {
            Some(version) if self.profile.version <= version => Err(Error::Stale),
            _ => Ok(()),
        }
    }
}

const POW_SALT: &[u8] = b"dyapp-profile-pow";
const HEARTBEAT_POW_SALT: &[u8] = b"dyapp-heartbeat-pow";

/// Leading zero bits of Argon2id(key ‖ nonce): the profile proof of work. 8 MiB, one pass.
pub fn pow_leading_bits(key: &[u8; 32], nonce: u64) -> u32 {
    let mut password = [0u8; 40];
    password[..32].copy_from_slice(key);
    password[32..].copy_from_slice(&nonce.to_le_bytes());
    argon_bits(POW_SALT, &password)
}

/// Leading zero bits of the heartbeat proof for UTC `day` (days since the epoch).
pub fn heartbeat_pow_bits(key: &[u8; 32], day: u32, nonce: u64) -> u32 {
    let mut password = [0u8; 44];
    password[..32].copy_from_slice(key);
    password[32..36].copy_from_slice(&day.to_be_bytes());
    password[36..].copy_from_slice(&nonce.to_be_bytes());
    argon_bits(HEARTBEAT_POW_SALT, &password)
}

/// [`solve`] for the heartbeat proof of `day`.
pub fn solve_heartbeat(
    key: &[u8; 32],
    day: u32,
    bits: u32,
    start: u64,
    tries: u64,
) -> Result<u64, u64> {
    let end = start.saturating_add(tries);
    (start..end)
        .find(|n| heartbeat_pow_bits(key, day, *n) >= bits)
        .ok_or(end)
}

/// Leading zero bits of Argon2id(password) with `salt`: 8 MiB, one pass, one lane.
fn argon_bits(salt: &[u8], password: &[u8]) -> u32 {
    let params = Params::new(8192, 1, 1, Some(32)).expect("valid Argon2 parameters");
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon
        .hash_password_into(password, salt, &mut out)
        .expect("fixed-size Argon2 input");
    let mut bits = 0;
    for byte in out {
        bits += byte.leading_zeros();
        if byte != 0 {
            break;
        }
    }
    bits
}

/// Tries nonces `start..start + tries`: `Ok` with the first reaching `bits`, else `Err` with the
/// nonce to resume from.
pub fn solve(key: &[u8; 32], bits: u32, start: u64, tries: u64) -> Result<u64, u64> {
    let end = start.saturating_add(tries);
    (start..end)
        .find(|n| pow_leading_bits(key, *n) >= bits)
        .ok_or(end)
}

/// Version of a record that was verified before it was stored.
pub fn stored_version(record: &SignedRecord) -> Result<u64, Error> {
    Profile::decode(record.payload.as_slice())
        .map(|profile| profile.version)
        .map_err(|_| Error::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(version: u64) -> Profile {
        Profile {
            version,
            age: 34,
            country: "DE".into(),
            income_from: Some(50_000),
            income_to: Some(70_000),
            place: "Berlin".into(),
            interests: vec!["hiking".into()],
            ..Profile::default()
        }
    }

    #[test]
    fn highest_valid_version_wins() {
        let owner = Identity::generate();
        let v1 = sample(1).sign(&owner);
        let v2 = sample(2).sign(&owner);

        let accepted = accept(&v1, None).unwrap();
        assert_eq!(accepted.peer_id, owner.peer_id());
        assert_eq!(accepted.profile, sample(1));
        assert!(accept(&v2, Some(1)).is_ok());
        assert_eq!(accept(&v1, Some(2)), Err(Error::Stale));
        assert_eq!(accept(&v2, Some(2)), Err(Error::Stale));
        assert_eq!(stored_version(&v2), Ok(2));
    }

    #[test]
    fn tombstone_supersedes_and_must_be_empty() {
        let owner = Identity::generate();
        let tombstone = Profile::tombstone(3, 0, 0).sign(&owner);
        assert!(accept(&tombstone, Some(2)).unwrap().profile.deleted);
        assert_eq!(accept(&sample(2).sign(&owner), Some(3)), Err(Error::Stale));

        let mut leaky = sample(4);
        leaky.deleted = true;
        assert_eq!(
            verify(&leaky.sign(&owner)),
            Err(Error::Invalid("tombstone carries data"))
        );
    }

    #[test]
    fn heartbeat_pow_depends_on_the_day() {
        let key = [7; 32];
        let nonce = solve_heartbeat(&key, 20_000, 4, 0, 10_000).unwrap();
        assert!(heartbeat_pow_bits(&key, 20_000, nonce) >= 4);
        // Other days hash differently: at 4 bits one in 16 matches, so 4 days all matching is
        // a 1 in 65536 accident.
        assert!((1..=4).any(|d| heartbeat_pow_bits(&key, 20_000 + d, nonce) < 4));
    }

    #[test]
    fn pow_is_pinned_and_solvable() {
        // Pinned format: change only with a protocol version bump. The first 6-bit nonce also
        // pins every hash before it.
        assert_eq!(pow_leading_bits(&[7; 32], 42), 1);
        assert_eq!(solve(&[7; 32], 6, 0, 10_000), Ok(154));
        let key = [9; 32];
        let nonce = solve(&key, 4, 0, 10_000).unwrap();
        assert!(pow_leading_bits(&key, nonce) >= 4);
        // `nonce` is the first match: a window ending before it fails and says where to resume.
        assert_eq!(solve(&key, 4, 0, nonce), Err(nonce));
        assert_eq!(solve(&key, 4, nonce, 1), Ok(nonce));
    }

    #[test]
    fn tombstone_keeps_pow() {
        let owner = Identity::generate();
        let tombstone = Profile::tombstone(3, 77, 11).sign(&owner);
        let verified = verify(&tombstone).unwrap().profile;
        assert_eq!((verified.pow_nonce, verified.pow_bits), (77, 11));
    }

    #[test]
    #[ignore = "benchmark: cargo test -p dyapp-profile --release -- --ignored --nocapture"]
    fn pow_timing() {
        let start = std::time::Instant::now();
        for nonce in 0..20 {
            pow_leading_bits(&[1; 32], nonce);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / 20.0;
        println!(
            "{ms:.1} ms per hash, ~{:.0} s for 11 bits",
            ms * 2048.0 / 1000.0
        );
    }

    #[test]
    fn rejects_bad_signature_and_content() {
        let owner = Identity::generate();
        let mut forged = sample(1).sign(&owner);
        forged.public_key = Identity::generate().public_key().to_vec();
        assert!(matches!(verify(&forged), Err(Error::Signature(_))));

        let mut reversed = sample(1);
        reversed.income_from = Some(80_000);
        assert!(verify(&reversed.sign(&owner)).is_err());

        let mut country = sample(1);
        country.country = "Germany".into();
        assert!(verify(&country.sign(&owner)).is_err());

        let mut place = sample(1);
        place.place = "я".repeat(MAX_PLACE_CHARS);
        assert!(verify(&place.sign(&owner)).is_ok());
        place.place.push('я');
        assert!(verify(&place.sign(&owner)).is_err());
        place.place = "Mitte\n".into();
        assert!(verify(&place.sign(&owner)).is_err());

        assert!(verify(&sample(0).sign(&owner)).is_err());

        let photo = Photo {
            full: vec![vec![1; 32], vec![2; 32]],
            thumbnail: vec![3; 32],
        };
        let mut photos = sample(1);
        photos.photos = vec![photo.clone(); MAX_PHOTOS];
        assert!(verify(&photos.sign(&owner)).is_ok());
        photos.photos.push(photo.clone());
        assert!(verify(&photos.sign(&owner)).is_err());
        for bad in [
            Photo {
                thumbnail: vec![],
                ..photo.clone()
            },
            Photo {
                full: vec![],
                ..photo.clone()
            },
            Photo {
                full: vec![vec![1; 31]],
                ..photo.clone()
            },
            Photo {
                full: vec![vec![1; 32]; MAX_PHOTO_BLOBS + 1],
                ..photo
            },
        ] {
            photos.photos = vec![bad];
            assert!(verify(&photos.sign(&owner)).is_err());
        }

        let garbage = owner.sign(Domain::Profile, vec![0xff; 4]);
        assert_eq!(verify(&garbage), Err(Error::Decode));
    }
}
