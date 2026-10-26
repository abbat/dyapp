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
/// Highest `education_level`, a doctorate.
pub const MAX_EDUCATION_LEVEL: u32 = 6;
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
pub use birth_date::{BirthBounds, CivilDate};
mod scales;
pub use scales::{
    scale, CareerAmbition, EmploymentStatus, FinancialPhilosophy, FitnessLevel, Gender,
    Orientation, RelationshipGoal, RolePreference, Scale, ScaleOption, Zodiac, SCALES,
};

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

    /// Re-encodes an edited profile without discarding fields this client does not know.
    /// Untouched photos retain their original bytes, including unknown nested fields.
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
        if self.version == 0 {
            return Err(Error::Invalid("version must start at 1"));
        }
        if self.deleted && *self != Self::tombstone(self.version, self.pow_nonce, self.pow_bits) {
            return Err(Error::Invalid("tombstone carries data"));
        }
        if !self.valid_birth_parts() {
            return Err(Error::Invalid("invalid birth date"));
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
        if self.education_level > MAX_EDUCATION_LEVEL {
            return Err(Error::Invalid("education level is not on the scale"));
        }
        for (value, maximum) in [
            (self.gender, Gender::MAX),
            (self.orientation, Orientation::MAX),
            (self.relationship_goal, RelationshipGoal::MAX),
            (self.role_preference, RolePreference::MAX),
            (self.career_ambition, CareerAmbition::MAX),
            (self.financial_philosophy, FinancialPhilosophy::MAX),
            (self.employment_status, EmploymentStatus::MAX),
            (self.fitness_level, FitnessLevel::MAX),
            (self.zodiac, Zodiac::MAX),
        ] {
            if value > maximum {
                return Err(Error::Invalid("categorical field is not on the scale"));
            }
        }
        if self.place.chars().count() > MAX_PLACE_CHARS || !is_clean_text(&self.place) {
            return Err(Error::Invalid("place is too long or not printable"));
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
        if self.devices.len() > MAX_DEVICES {
            return Err(Error::Invalid("too many devices"));
        }
        let mut previous = None;
        let mut mls_keys = HashSet::new();
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
        }
        Ok(())
    }
}

/// Whether `text` may appear in a profile string: NFC, with no control, format (bidi, zero-width)
/// or private-use characters except ZWNJ and ZWJ, which emoji sequences and several scripts need.
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
mod tests {
    use super::*;

    fn sample(version: u64) -> Profile {
        Profile {
            version,
            birth_year: Some(1992),
            country: "DE".into(),
            income_from: Some(50_000),
            income_to: Some(70_000),
            place: "Berlin".into(),
            interests: vec!["hiking".into()],
            ..Profile::default()
        }
    }

    #[test]
    fn edited_profile_keeps_unknown_top_level_fields() {
        let mut original = sample(1).encode_to_vec();
        let unknown_varint = [0xa0, 0x06, 0x07]; // tag 100
        let unknown_bytes = [0xaa, 0x06, 0x02, 0xde, 0xad]; // tag 101
        original.extend_from_slice(&unknown_varint);
        original.extend_from_slice(&unknown_bytes);

        let mut edited = Profile::decode(original.as_slice()).unwrap();
        edited.birth_month = Some(5);
        let output = edited.encode_preserving(&original).unwrap();
        assert!(output
            .windows(unknown_varint.len())
            .any(|w| w == unknown_varint));
        assert!(output
            .windows(unknown_bytes.len())
            .any(|w| w == unknown_bytes));
        assert_eq!(
            Profile::decode(output.as_slice()).unwrap().birth_month,
            Some(5)
        );
        assert_eq!(
            sample(2)
                .encode_preserving(&sample(1).encode_to_vec())
                .unwrap(),
            sample(2).encode_to_vec()
        );
    }

    #[test]
    fn edited_profile_keeps_untouched_photo_bytes() {
        let photo = Photo {
            full: vec![vec![1; 32]],
            thumbnail: vec![2; 32],
        };
        let mut nested = photo.encode_to_vec();
        nested.extend_from_slice(&[0xa0, 0x06, 0x07]); // unknown tag 100
        let mut original = sample(1).encode_to_vec();
        original.extend_from_slice(&[0xaa, 0x01, nested.len() as u8]); // tag 21
        original.extend_from_slice(&nested);

        let mut edited = Profile::decode(original.as_slice()).unwrap();
        edited.birth_month = Some(5);
        let output = edited.encode_preserving(&original).unwrap();
        assert!(output.windows(nested.len()).any(|w| w == nested));
        assert_eq!(
            Profile::decode(output.as_slice()).unwrap().birth_month,
            Some(5)
        );

        edited.photos = vec![Photo {
            thumbnail: vec![3; 32],
            ..photo
        }];
        let output = edited.encode_preserving(&original).unwrap();
        assert!(!output.windows(nested.len()).any(|w| w == nested));
        assert_eq!(
            Profile::decode(output.as_slice()).unwrap().photos,
            edited.photos
        );
    }

    #[test]
    fn field_spans_cover_every_wire_type_and_reject_bad_keys() {
        let max = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01];
        let long_zero = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let mut bytes = vec![0x08];
        bytes.extend_from_slice(&max);
        bytes.push(0x08);
        bytes.extend_from_slice(&long_zero);
        bytes.extend_from_slice(&[0x11, 1, 2, 3, 4, 5, 6, 7, 8]); // tag 2, fixed64
        bytes.extend_from_slice(&[0x1d, 1, 2, 3, 4]); // tag 3, fixed32
        assert_eq!(
            fields(&bytes),
            Ok(vec![(1, 0..11), (1, 11..22), (2, 22..31), (3, 31..36)])
        );

        let mut overflow = vec![0x08];
        overflow.extend_from_slice(&max[..9]);
        overflow.push(0x02);
        assert_eq!(fields(&overflow), Err(Error::Decode));
        assert_eq!(fields(&[0x00, 0x00]), Err(Error::Decode)); // tag 0
        let tag_too_big = [0x80, 0x80, 0x80, 0x80, 0x10, 0x00]; // tag 1 << 29
        assert_eq!(fields(&tag_too_big), Err(Error::Decode));
    }

    #[test]
    fn malformed_original_profile_cannot_be_preserved() {
        assert_eq!(
            Profile::default().encode_preserving(&[0xff]),
            Err(Error::Decode)
        );
    }

    #[test]
    fn device_directory_checks_owner_keys_order_and_signature() {
        let owner = Identity::generate();
        let first = Identity::generate();
        let second = Identity::generate();
        let mls_first = Identity::generate();
        let mls_second = Identity::generate();
        let certificate = |device: &Identity, mls: &Identity| {
            owner.sign(
                Domain::DeviceCertificate,
                DeviceCertificate {
                    device_key: device.public_key().to_vec(),
                    mls_signature_key: mls.public_key().to_vec(),
                    generation: 1,
                }
                .encode_to_vec(),
            )
        };
        let mut devices = vec![
            certificate(&first, &mls_first),
            certificate(&second, &mls_second),
        ];
        devices.sort_by_key(|signed| {
            let cert = DeviceCertificate::decode(signed.payload.as_slice()).unwrap();
            key_hash(&cert.device_key.try_into().unwrap())
        });
        let mut profile = sample(1);
        profile.devices = devices;
        assert!(verify(&profile.sign(&owner)).is_ok());

        profile.devices.reverse();
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("duplicate or unsorted devices"))
        );
        profile.devices.reverse();
        profile.devices[1] = profile.devices[0].clone();
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("duplicate or unsorted devices"))
        );
        profile.devices[0].signature[0] ^= 1;
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("invalid device certificate signature"))
        );
        profile.devices[0] = Identity::generate().sign(
            Domain::DeviceCertificate,
            DeviceCertificate {
                device_key: first.public_key().to_vec(),
                mls_signature_key: mls_first.public_key().to_vec(),
                generation: 1,
            }
            .encode_to_vec(),
        );
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("invalid device certificate signature"))
        );
    }

    #[test]
    fn content_rules_hold_at_their_boundaries() {
        let owner = Identity::generate();
        let invalid = |profile: &Profile, reason| {
            assert_eq!(
                verify(&profile.sign(&owner)),
                Err(Error::Invalid(reason)),
                "{reason}"
            );
        };
        let mut country = sample(1);
        for code in ["de", "DEU"] {
            country.country = code.into();
            invalid(&country, "country is not an ISO 3166-1 alpha-2 code");
        }
        let mut income = sample(1);
        income.income_to = income.income_from;
        assert!(verify(&income.sign(&owner)).is_ok());

        let certificate = |device: [u8; 32], mls: [u8; 32], generation| {
            owner.sign(
                Domain::DeviceCertificate,
                DeviceCertificate {
                    device_key: device.to_vec(),
                    mls_signature_key: mls.to_vec(),
                    generation,
                }
                .encode_to_vec(),
            )
        };
        let mut keys: Vec<_> = (0..=MAX_DEVICES)
            .map(|_| {
                (
                    Identity::generate().public_key(),
                    Identity::generate().public_key(),
                )
            })
            .collect();
        keys.sort_by_key(|(device, _)| key_hash(device));
        let mut devices = sample(1);
        devices.devices = keys.iter().map(|&(d, m)| certificate(d, m, 1)).collect();
        invalid(&devices, "too many devices");
        devices.devices.pop();
        assert!(verify(&devices.sign(&owner)).is_ok());

        let (device, mls) = keys[0];
        let own = owner.public_key();
        for (device, mls, generation) in [
            (device, mls, 0),
            (device, device, 1),
            (own, mls, 1),
            (device, own, 1),
        ] {
            devices.devices = vec![certificate(device, mls, generation)];
            invalid(&devices, "invalid device key or generation");
        }

        let at_limit = owner.sign(Domain::Profile, vec![0xff; MAX_PAYLOAD_LEN]);
        assert_eq!(verify(&at_limit), Err(Error::Decode));
        let over = owner.sign(Domain::Profile, vec![0xff; MAX_PAYLOAD_LEN + 1]);
        assert_eq!(verify(&over), Err(Error::Invalid("profile too large")));
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
    fn optional_career_fields_are_signed_and_validated() {
        let owner = Identity::generate();
        let mut profile = sample(1);
        assert!(profile.profession.is_empty());
        assert!(verify(&profile.sign(&owner)).is_ok());

        profile.profession = "Engineer".into();
        profile.company = "Example Co".into();
        profile.industry = "Technology".into();
        profile.career_stage = "Senior".into();
        profile.work_life_balance = "Flexible".into();
        assert_eq!(verify(&profile.sign(&owner)).unwrap().profile, profile);

        for invalid in ["x".repeat(MAX_FIELD_CHARS + 1), "bad\u{200B}".into()] {
            for field in 0..5 {
                let mut bad = profile.clone();
                *[
                    &mut bad.profession,
                    &mut bad.company,
                    &mut bad.industry,
                    &mut bad.career_stage,
                    &mut bad.work_life_balance,
                ][field] = invalid.clone();
                assert!(verify(&bad.sign(&owner)).is_err());
            }
        }

        profile.deleted = true;
        assert_eq!(
            verify(&profile.sign(&owner)),
            Err(Error::Invalid("tombstone carries data"))
        );
    }

    #[test]
    fn profile_strings_are_nfc_without_hidden_characters_and_bounded() {
        for text in [
            "Berlin",
            "Москва",
            "東京",
            "🙂",
            "👨\u{200D}👩\u{200D}👧",
            "می\u{200C}خواهم",
        ] {
            assert!(is_clean_text(text), "{text}");
        }
        for text in [
            "a\u{202E}b",
            "a\u{200B}b",
            "a\u{E000}b",
            "e\u{301}",
            "a\u{7}",
        ] {
            assert!(!is_clean_text(text), "{text:?}");
        }

        let owner = Identity::generate();
        let mut profile = sample(1);
        profile.place = "Mit\u{202E}te".into();
        assert!(verify(&profile.sign(&owner)).is_err());
        let mut profile = sample(1);
        profile.profession = "x".repeat(MAX_FIELD_CHARS);
        assert!(verify(&profile.sign(&owner)).is_ok());
        profile.profession.push('x');
        assert!(verify(&profile.sign(&owner)).is_err());
        profile.profession = "high\u{200B}".into();
        assert!(verify(&profile.sign(&owner)).is_err());
        let mut profile = sample(1);
        profile.education_level = MAX_EDUCATION_LEVEL;
        assert!(verify(&profile.sign(&owner)).is_ok());
        profile.education_level += 1;
        assert!(verify(&profile.sign(&owner)).is_err());
        let mut profile = sample(1);
        profile.interests = vec!["x".repeat(MAX_FIELD_CHARS); MAX_INTERESTS];
        assert!(verify(&profile.sign(&owner)).is_ok());
        profile.interests.push("chess".into());
        assert!(verify(&profile.sign(&owner)).is_err());
        profile.interests = vec!["x".repeat(MAX_FIELD_CHARS + 1)];
        assert!(verify(&profile.sign(&owner)).is_err());
        profile.interests = vec!["\u{E000}".into()];
        assert!(verify(&profile.sign(&owner)).is_err());

        let mut profile = sample(1);
        profile.name = "Анна 🙂".into();
        assert_eq!(verify(&profile.sign(&owner)).unwrap().profile, profile);
        profile.name = "x".repeat(MAX_FIELD_CHARS + 1);
        assert!(verify(&profile.sign(&owner)).is_err());
        profile.name = "Ann\u{202E}a".into();
        assert!(verify(&profile.sign(&owner)).is_err());
    }

    #[test]
    fn closed_scales_accept_zero_and_reject_unknown_values() {
        fn field(profile: &mut Profile, tag: u32) -> &mut u32 {
            match tag {
                36 => &mut profile.gender,
                37 => &mut profile.orientation,
                38 => &mut profile.relationship_goal,
                39 => &mut profile.role_preference,
                40 => &mut profile.career_ambition,
                41 => &mut profile.financial_philosophy,
                42 => &mut profile.employment_status,
                43 => &mut profile.fitness_level,
                44 => &mut profile.zodiac,
                _ => unreachable!(),
            }
        }
        let owner = Identity::generate();
        let mut profile = sample(1);
        for scale in SCALES {
            *field(&mut profile, scale.field) = scale.max;
            assert!(verify(&profile.sign(&owner)).is_ok(), "{}", scale.key);
            *field(&mut profile, scale.field) = scale.max + 1;
            assert_eq!(
                verify(&profile.sign(&owner)),
                Err(Error::Invalid("categorical field is not on the scale")),
                "{}",
                scale.key
            );
            *field(&mut profile, scale.field) = 0;
            assert!(verify(&profile.sign(&owner)).is_ok(), "{}", scale.key);
        }
        assert!(verify(&Profile::tombstone(2, 0, 0).sign(&owner)).is_ok());
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

        let mut gender = sample(1);
        gender.gender = Gender::Woman.code();
        let verified = verify(&gender.sign(&owner)).unwrap();
        assert_eq!(verified.profile.gender, Gender::Woman.code());

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
