//! Versioned app PIN verifier. Secure storage and attempt admission are separate concerns.

use argon2::{Algorithm, Argon2, Params, Version};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const FORMAT: u8 = 1;
const SALT_LEN: usize = 16;
const TAG_LEN: usize = 32;
const RECORD_LEN: usize = 1 + SALT_LEN + TAG_LEN;

/// PIN input, stored record or verifier failure; messages never contain supplied PINs.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Only six to 64 ASCII decimal digits are accepted.
    #[error("PIN must contain 6 to 64 decimal digits")]
    InvalidPin,
    /// The record has an unsupported size or version.
    #[error("Invalid PIN verifier record")]
    InvalidRecord,
    /// Randomness or Argon2 computation failed.
    #[error("PIN verifier unavailable")]
    Unavailable,
}

fn valid_pin(pin: &str) -> bool {
    (6..=64).contains(&pin.len()) && pin.bytes().all(|byte| byte.is_ascii_digit())
}

fn tag(pin: &str, salt: &[u8]) -> Result<[u8; TAG_LEN], Error> {
    let params = Params::new(64 * 1024, 3, 4, Some(TAG_LEN)).map_err(|_| Error::Unavailable)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0; TAG_LEN];
    argon
        .hash_password_into(pin.as_bytes(), salt, &mut output)
        .map_err(|_| Error::Unavailable)?;
    Ok(output)
}

/// Hash an owned PIN into format 1, a random 16-byte salt and a 32-byte Argon2id tag.
/// The owned plaintext and temporary tag are zeroized. Protect the resulting verifier in
/// device-only storage; this function does not enforce retry limits.
/// Run off the UI thread: the existing KDF uses 64 MiB, three passes and four lanes.
///
/// # Errors
/// Returns an input error for non-ASCII/non-decimal or out-of-range PINs, or an unavailable
/// verifier error if randomness or KDF computation fails.
pub fn hash(pin: String) -> Result<Vec<u8>, Error> {
    let pin = Zeroizing::new(pin);
    if !valid_pin(&pin) {
        return Err(Error::InvalidPin);
    }
    let mut salt = [0; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|_| Error::Unavailable)?;
    let tag = Zeroizing::new(tag(&pin, &salt)?);
    let mut record = Vec::with_capacity(RECORD_LEN);
    record.push(FORMAT);
    record.extend_from_slice(&salt);
    record.extend_from_slice(&tag[..]);
    Ok(record)
}

/// Verify an owned PIN against the existing format-1 record with a constant-time tag comparison.
/// Zeroizes owned plaintext and the computed tag. Wrong PIN returns false; attempt admission
/// and durable failure counting belong to the app-lock state machine.
///
/// # Errors
/// Returns an input error, an invalid record error for unsupported size/version, or a verifier
/// computation error; it never returns PINs or record bytes in a diagnostic.
pub fn verify(pin: String, record: &[u8]) -> Result<bool, Error> {
    let pin = Zeroizing::new(pin);
    if !valid_pin(&pin) {
        return Err(Error::InvalidPin);
    }
    if record.len() != RECORD_LEN || record[0] != FORMAT {
        return Err(Error::InvalidRecord);
    }
    let computed = Zeroizing::new(tag(&pin, &record[1..=SALT_LEN])?);
    Ok(bool::from(computed[..].ct_eq(&record[1 + SALT_LEN..])))
}

#[cfg(test)]
mod tests;
