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

/// Every field is public. Empty strings, `None` and an age of 0 mean "not published".
#[derive(Clone, PartialEq, prost::Message)]
pub struct Profile {
    #[prost(uint64, tag = "1")]
    pub version: u64,
    #[prost(bool, tag = "2")]
    pub deleted: bool,
    #[prost(uint32, tag = "3")]
    pub age: u32,
    /// ISO 3166-1 alpha-2 code; income is compared only within one country.
    #[prost(string, tag = "4")]
    pub country: String,
    // Tag 5 held coordinates; do not reuse it.
    /// Free income range, no currency or brackets.
    #[prost(uint64, optional, tag = "6")]
    pub income_from: Option<u64>,
    #[prost(uint64, optional, tag = "7")]
    pub income_to: Option<u64>,
    #[prost(bool, optional, tag = "8")]
    pub has_kids: Option<bool>,
    #[prost(bool, optional, tag = "9")]
    pub wants_more_kids: Option<bool>,
    #[prost(string, tag = "10")]
    pub relationship_goal: String,
    #[prost(string, tag = "11")]
    pub career_ambition: String,
    #[prost(string, tag = "12")]
    pub education: String,
    #[prost(string, tag = "13")]
    pub orientation: String,
    #[prost(string, tag = "14")]
    pub role_preference: String,
    #[prost(string, tag = "15")]
    pub zodiac: String,
    #[prost(string, tag = "16")]
    pub financial_philosophy: String,
    #[prost(string, tag = "17")]
    pub employment_status: String,
    #[prost(string, tag = "18")]
    pub fitness_level: String,
    #[prost(string, repeated, tag = "19")]
    pub interests: Vec<String>,
    /// City or district within `country`, no coordinates; matched exactly.
    #[prost(string, tag = "20")]
    pub place: String,
}

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

        let garbage = owner.sign(Domain::Profile, vec![0xff; 4]);
        assert_eq!(verify(&garbage), Err(Error::Decode));
    }
}
