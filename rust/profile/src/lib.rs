//! Public profile: a plaintext document signed by its owner's identity key.
//!
//! Each published version carries a version number; the highest validly signed version wins.
//! Deleting a profile publishes a tombstone: a signed version with `deleted` set and every other
//! field empty. Nodes keep the tombstone so older versions are not re-imported.

use dyapp_identity::{Domain, Identity, SignedRecord};
use prost::Message;

/// Largest accepted profile payload. Media are not part of it: the profile only links to blobs.
pub const MAX_PAYLOAD_LEN: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error(transparent)]
    Signature(#[from] dyapp_identity::Error),
    #[error("malformed profile")]
    Decode,
    #[error("invalid profile: {0}")]
    Invalid(&'static str),
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
    include!(concat!(env!("OUT_DIR"), "/dyapp.profile.rs"));
}
pub use generated::{Photo, Profile};

impl Profile {
    pub fn tombstone(version: u64) -> Self {
        Self {
            version,
            deleted: true,
            ..Self::default()
        }
    }

    pub fn sign(&self, identity: &Identity) -> SignedRecord {
        identity.sign(Domain::Profile, self.encode_to_vec())
    }

    fn validate(&self) -> Result<(), Error> {
        if self.version == 0 {
            return Err(Error::Invalid("version must start at 1"));
        }
        if self.deleted && *self != Self::tombstone(self.version) {
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
    pub peer_id: String,
    pub profile: Profile,
}

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
    pub fn check_newer(&self, current: Option<u64>) -> Result<(), Error> {
        match current {
            Some(version) if self.profile.version <= version => Err(Error::Stale),
            _ => Ok(()),
        }
    }
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
        let tombstone = Profile::tombstone(3).sign(&owner);
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
