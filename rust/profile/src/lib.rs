//! Public profile: a plaintext document signed by its owner's identity key.
//!
//! Each published version carries a version number; the highest validly signed version wins.
//! Deleting a profile publishes a tombstone: a signed version with `deleted` set and every other
//! field but the proof of work empty. Nodes keep the tombstone so older versions are not
//! re-imported.

use argon2::{Algorithm, Argon2, Params, Version};
use dyapp_identity::{key_hash, Domain, Identity, SignedRecord};
use icu_normalizer::ComposingNormalizerBorrowed;
use icu_properties::{props::GeneralCategory, CodePointMapData};
use prost::Message;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};

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
/// Longest accepted short text field, in Unicode code points.
pub const MAX_FIELD_CHARS: usize = 64;
/// Longest accepted `about`, in Unicode code points.
pub const MAX_ABOUT_CHARS: usize = 500;
/// `Profile` field number of `height`.
pub const HEIGHT_FIELD: u32 = 46;
/// Shortest accepted `height`, in whole centimetres.
pub const MIN_HEIGHT_CM: u32 = 50;
/// Tallest accepted `height`, in whole centimetres.
pub const MAX_HEIGHT_CM: u32 = 272;
/// Most `seeking_genders` a profile lists.
pub const MAX_SEEKING_GENDERS: usize = 16;
/// Lowest accepted partner age bound.
pub const MIN_SEEKING_AGE: u32 = 18;
/// Highest accepted partner age bound.
pub const MAX_SEEKING_AGE: u32 = 120;
/// Most `interests` a profile lists; each is at most `MAX_FIELD_CHARS` long.
pub const MAX_INTERESTS: usize = 32;
/// Most photos a profile links.
pub const MAX_PHOTOS: usize = 16;
/// Most blobs one full photo is split into.
pub const MAX_PHOTO_BLOBS: usize = 64;
/// Most devices an owner can list in one profile.
pub const MAX_DEVICES: usize = 64;

mod generated {
    // prost emits no docs for messages without proto comments.
    #![expect(missing_docs, reason = "prost-generated messages omit documentation")]
    include!(concat!(env!("OUT_DIR"), "/dyapp.profile.rs"));
}
pub use generated::{DeviceCertificate, Photo, Profile};

mod birth_date;
pub mod search_input;
pub use birth_date::{BirthBounds, CivilDate};
mod scales;
pub use scales::{
    scale, stated, CareerAmbition, Drinking, EducationLevel, EmploymentStatus, FinancialPhilosophy,
    FitnessLevel, Gender, MaritalStatus, Orientation, Pets, RelationshipGoal, RolePreference,
    Scale, ScaleOption, Smoking, Tattoos, Zodiac, SCALES,
};

/// Whether `height` is unset or within [`MIN_HEIGHT_CM`]..=[`MAX_HEIGHT_CM`].
#[must_use]
pub fn valid_height(height: Option<u32>) -> bool {
    height.is_none_or(|cm| (MIN_HEIGHT_CM..=MAX_HEIGHT_CM).contains(&cm))
}

/// Whether `about` fits [`MAX_ABOUT_CHARS`] and each of its lines passes [`is_clean_text`].
#[must_use]
pub fn valid_about(about: &str) -> bool {
    about.chars().count() <= MAX_ABOUT_CHARS && about.split('\n').all(is_clean_text)
}

/// Whether the genders hold no zero or repeat and fit [`MAX_SEEKING_GENDERS`], and the age bounds
/// are in range and not reversed. Unknown gender codes pass, as for every scale on nodes.
#[must_use]
pub fn valid_seeking(genders: &[u32], age_from: Option<u32>, age_to: Option<u32>) -> bool {
    let age =
        |age: Option<u32>| age.is_none_or(|a| (MIN_SEEKING_AGE..=MAX_SEEKING_AGE).contains(&a));
    let mut seen = HashSet::new();
    genders.len() <= MAX_SEEKING_GENDERS
        && genders.iter().all(|&code| code != 0 && seen.insert(code))
        && age(age_from)
        && age(age_to)
        && age_from.zip(age_to).is_none_or(|(from, to)| from <= to)
}

impl Profile {
    /// A deletion marker: version and proof of work set, every other field empty.
    #[must_use]
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
    #[must_use]
    pub fn sign(&self, identity: &Identity) -> SignedRecord {
        identity.sign(Domain::Profile, self.encode_to_vec())
    }

    /// Re-encodes an edited profile without discarding fields this client does not know.
    /// Untouched photos retain their original bytes, including unknown nested fields.
    ///
    /// # Errors
    /// Returns [`Error::Decode`] if the original profile or its protobuf field spans
    /// cannot be decoded.
    pub fn encode_preserving(&self, original: &[u8]) -> Result<Vec<u8>, Error> {
        let previous = Self::decode(original).map_err(|_| Error::Decode)?;
        let known: HashSet<u32> = fields(&previous.encode_to_vec())?
            .into_iter()
            .map(|(tag, _)| tag)
            .collect();
        let keep_photos = self.photos == previous.photos;
        let mut encoded = Vec::new();
        for (tag, span) in fields(original)? {
            if !known.contains(&tag) || (keep_photos && tag == 21) {
                encoded.extend_from_slice(&original[span]);
            }
        }
        let edited = self.encode_to_vec();
        if keep_photos {
            for (tag, span) in fields(&edited)? {
                if tag != 21 {
                    encoded.extend_from_slice(&edited[span]);
                }
            }
        } else {
            encoded.extend_from_slice(&edited);
        }
        Ok(encoded)
    }

    fn validate(&self, owner_key: &[u8]) -> Result<(), Error> {
        self.validate_fields()?;
        self.validate_text()?;
        self.validate_photos()?;
        self.validate_devices(owner_key)
    }

    fn validate_fields(&self) -> Result<(), Error> {
        if self.version == 0 {
            return Err(Error::Invalid("version must start at 1"));
        }
        if self.deleted && *self != Self::tombstone(self.version, self.pow_nonce, self.pow_bits) {
            return Err(Error::Invalid("tombstone carries data"));
        }
        if !self.valid_birth_parts() {
            return Err(Error::Invalid("invalid birth date"));
        }
        if !(self.country.is_empty()
            || self.country.len() == 2 && self.country.bytes().all(|b| b.is_ascii_uppercase()))
        {
            return Err(Error::Invalid("country is not an ISO 3166-1 alpha-2 code"));
        }
        if let (Some(from), Some(to)) = (self.income_from, self.income_to) {
            if from > to {
                return Err(Error::Invalid("income range is reversed"));
            }
        }
        if !valid_height(self.height) {
            return Err(Error::Invalid("height is out of range"));
        }
        if !valid_seeking(
            &self.seeking_genders,
            self.seeking_age_from,
            self.seeking_age_to,
        ) {
            return Err(Error::Invalid("seeking genders or ages are invalid"));
        }
        if self.not_looking
            && (!self.seeking_genders.is_empty()
                || self.seeking_age_from.is_some()
                || self.seeking_age_to.is_some())
        {
            return Err(Error::Invalid("not looking but states who is sought"));
        }
        Ok(())
    }

    fn validate_text(&self) -> Result<(), Error> {
        if self.place.chars().count() > MAX_PLACE_CHARS || !is_clean_text(&self.place) {
            return Err(Error::Invalid("place is too long or not printable"));
        }
        if !valid_about(&self.about) {
            return Err(Error::Invalid("about is too long or not printable"));
        }
        let fields = [
            &self.name,
            &self.profession,
            &self.company,
            &self.industry,
            &self.career_stage,
            &self.work_life_balance,
        ];
        let short = |text: &&String| text.chars().count() <= MAX_FIELD_CHARS && is_clean_text(text);
        if !fields.iter().all(short) {
            return Err(Error::Invalid("a field is too long or not printable"));
        }
        if self.interests.len() > MAX_INTERESTS || !self.interests.iter().all(|i| short(&i)) {
            return Err(Error::Invalid(
                "too many interests, or one is too long or not printable",
            ));
        }
        if self.interests.iter().any(|i| *i != normalize_interest(i)) {
            return Err(Error::Invalid("interest is not normalized"));
        }
        Ok(())
    }

    fn validate_photos(&self) -> Result<(), Error> {
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

    fn validate_devices(&self, owner_key: &[u8]) -> Result<(), Error> {
        if self.devices.len() > MAX_DEVICES {
            return Err(Error::Invalid("too many devices"));
        }
        let mut previous = None;
        let mut mls_keys = HashSet::new();
        let mut listed = HashSet::new();
        for signed in &self.devices {
            if signed.public_key != owner_key || signed.verify(Domain::DeviceCertificate).is_err() {
                return Err(Error::Invalid("invalid device certificate signature"));
            }
            let certificate = DeviceCertificate::decode(signed.payload.as_slice())
                .map_err(|_| Error::Invalid("malformed device certificate"))?;
            let (Ok(device), Ok(mls)) = (
                <[u8; 32]>::try_from(certificate.device_key.as_slice()),
                <[u8; 32]>::try_from(certificate.mls_signature_key.as_slice()),
            ) else {
                return Err(Error::Invalid("invalid device key length"));
            };
            if certificate.generation == 0
                || device == mls
                || device.as_slice() == owner_key
                || mls.as_slice() == owner_key
            {
                return Err(Error::Invalid("invalid device key or generation"));
            }
            let mailbox = key_hash(&device);
            if previous.is_some_and(|last| mailbox <= last) || !mls_keys.insert(mls) {
                return Err(Error::Invalid("duplicate or unsorted devices"));
            }
            previous = Some(mailbox);
            listed.insert(device);
        }
        let binds = |signed: &SignedRecord| {
            <[u8; 32]>::try_from(signed.public_key.as_slice())
                .is_ok_and(|key| listed.contains(&key))
                && signed.payload == owner_key
                && signed.verify(Domain::DeviceOwner).is_ok()
        };
        if self.device_owners.len() > self.devices.len() || !self.device_owners.iter().all(binds) {
            return Err(Error::Invalid("invalid device owner binding"));
        }
        Ok(())
    }
}

/// Canonical interest tag used by both profile publication and search: trim, lowercase, NFC.
#[must_use]
pub fn normalize_interest(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(&lower)
        .into_owned()
}

/// Whether `text` may appear in a profile string: NFC, with no control, format (bidi, zero-width)
/// or private-use characters except ZWNJ and ZWJ, which emoji sequences and several scripts need.
#[must_use]
pub fn is_clean_text(text: &str) -> bool {
    let category = CodePointMapData::<GeneralCategory>::new();
    ComposingNormalizerBorrowed::new_nfc().is_normalized(text)
        && text.chars().all(|c| {
            matches!(c, '\u{200C}' | '\u{200D}')
                || !matches!(
                    category.get(c),
                    GeneralCategory::Control
                        | GeneralCategory::Format
                        | GeneralCategory::PrivateUse
                )
        })
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
///
/// # Errors
/// Returns an error for an oversized payload, invalid signature, malformed protobuf
/// or a profile that violates the content rules.
pub fn verify(record: &SignedRecord) -> Result<VerifiedProfile, Error> {
    if record.payload.len() > MAX_PAYLOAD_LEN {
        return Err(Error::Invalid("profile too large"));
    }
    let peer_id = record.verify(Domain::Profile)?;
    let profile = Profile::decode(record.payload.as_slice()).map_err(|_| Error::Decode)?;
    profile.validate(&record.public_key)?;
    Ok(VerifiedProfile { peer_id, profile })
}

/// Verifies `record` and accepts it only if it is newer than `current`, the owner's stored
/// version (already verified when it was stored).
///
/// # Errors
/// Returns any error from [`verify`], or [`Error::Stale`] if the verified version
/// does not exceed `current`.
pub fn accept(record: &SignedRecord, current: Option<u64>) -> Result<VerifiedProfile, Error> {
    let verified = verify(record)?;
    verified.check_newer(current)?;
    Ok(verified)
}

impl VerifiedProfile {
    /// Checks whether this version is above `current`, the stored one.
    ///
    /// # Errors
    /// Returns [`Error::Stale`] when `current` is at least this profile's version.
    pub fn check_newer(&self, current: Option<u64>) -> Result<(), Error> {
        match current {
            Some(version) if self.profile.version <= version => Err(Error::Stale),
            _ => Ok(()),
        }
    }
}

/// Profile proof of work a publish computes and a search requires: the default search-node
/// minimum, until clients take it from node hints.
pub const POW_BITS: u32 = 11;

static REQUIRED_POW_BITS: AtomicU32 = AtomicU32::new(POW_BITS);

/// Overrides [`POW_BITS`] for this process: a test network sets it low.
pub fn set_required_pow_bits(bits: u32) {
    REQUIRED_POW_BITS.store(bits, Ordering::Relaxed);
}

/// Profile proof of work this process computes and requires.
#[must_use]
pub fn required_pow_bits() -> u32 {
    REQUIRED_POW_BITS.load(Ordering::Relaxed)
}

const POW_SALT: &[u8] = b"dyapp-profile-pow";
const HEARTBEAT_POW_SALT: &[u8] = b"dyapp-heartbeat-pow";

/// Leading zero bits of Argon2id(key ‖ nonce): the profile proof of work. 8 MiB, one pass.
#[must_use]
pub fn pow_leading_bits(key: &[u8; 32], nonce: u64) -> u32 {
    let mut password = [0u8; 40];
    password[..32].copy_from_slice(key);
    password[32..].copy_from_slice(&nonce.to_le_bytes());
    argon_bits(POW_SALT, &password)
}

/// Leading zero bits of the heartbeat proof for UTC `day` (days since the epoch).
#[must_use]
pub fn heartbeat_pow_bits(key: &[u8; 32], day: u32, nonce: u64) -> u32 {
    let mut password = [0u8; 44];
    password[..32].copy_from_slice(key);
    password[32..36].copy_from_slice(&day.to_be_bytes());
    password[36..].copy_from_slice(&nonce.to_be_bytes());
    argon_bits(HEARTBEAT_POW_SALT, &password)
}

/// [`solve`] for the heartbeat proof of `day`.
///
/// # Errors
/// Returns the next nonce to resume from if no nonce in the bounded search reaches
/// `bits`. The search end saturates at [`u64::MAX`].
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
///
/// # Errors
/// Returns the next nonce to resume from if the bounded search finds no solution.
/// The search end saturates at [`u64::MAX`].
pub fn solve(key: &[u8; 32], bits: u32, start: u64, tries: u64) -> Result<u64, u64> {
    let end = start.saturating_add(tries);
    (start..end)
        .find(|n| pow_leading_bits(key, *n) >= bits)
        .ok_or(end)
}

/// Version of a record that was verified before it was stored.
///
/// # Errors
/// Returns [`Error::Decode`] if the stored payload cannot be decoded as a profile.
pub fn stored_version(record: &SignedRecord) -> Result<u64, Error> {
    Profile::decode(record.payload.as_slice())
        .map(|profile| profile.version)
        .map_err(|_| Error::Decode)
}

// Return each complete protobuf field span so unknown fields can be copied unchanged.
fn fields(bytes: &[u8]) -> Result<Vec<(u32, std::ops::Range<usize>)>, Error> {
    fn varint(bytes: &[u8], cursor: &mut usize) -> Result<u64, Error> {
        let mut value = 0_u64;
        for index in 0..10 {
            let byte = *bytes.get(*cursor).ok_or(Error::Decode)?;
            *cursor += 1;
            if index == 9 && byte > 1 {
                return Err(Error::Decode);
            }
            value |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::Decode)
    }

    let mut cursor = 0;
    let mut spans = Vec::new();
    while cursor < bytes.len() {
        let start = cursor;
        let key = varint(bytes, &mut cursor)?;
        let tag = u32::try_from(key >> 3).map_err(|_| Error::Decode)?;
        if tag == 0 || tag >= 1 << 29 {
            return Err(Error::Decode);
        }
        match key & 7 {
            0 => {
                varint(bytes, &mut cursor)?;
            }
            1 => cursor = cursor.checked_add(8).ok_or(Error::Decode)?,
            2 => {
                let len =
                    usize::try_from(varint(bytes, &mut cursor)?).map_err(|_| Error::Decode)?;
                cursor = cursor.checked_add(len).ok_or(Error::Decode)?;
            }
            5 => cursor = cursor.checked_add(4).ok_or(Error::Decode)?,
            _ => return Err(Error::Decode),
        }
        if cursor > bytes.len() {
            return Err(Error::Decode);
        }
        spans.push((tag, start..cursor));
    }
    Ok(spans)
}

#[cfg(test)]
mod tests;
