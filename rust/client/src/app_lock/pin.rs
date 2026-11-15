//! Versioned app PIN verifier. Secure storage and attempt admission are separate concerns.

use argon2::{Algorithm, Argon2, Params, Version};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

const MIN_PIN_DIGITS: usize = 6;
const MAX_PIN_DIGITS: usize = 64;
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

/// Cheap input readiness only; it never verifies a secret or admits an unlock attempt.
#[must_use]
pub fn valid_pin(pin: &str) -> bool {
    (MIN_PIN_DIGITS..=MAX_PIN_DIGITS).contains(&pin.len())
        && pin.bytes().all(|byte| byte.is_ascii_digit())
}

/// Existing secure-field assistance; authentication APIs still reject invalid raw PINs.
/// The returned plaintext belongs only to the caller's transient secure field.
#[must_use]
pub fn normalize_input(pin: String) -> String {
    let pin = Zeroizing::new(pin);
    pin.chars()
        .filter(char::is_ascii_digit)
        .take(MAX_PIN_DIGITS)
        .collect()
}

/// Presentation outcome for a new PIN and its confirmation; never an authorization result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    /// The candidate does not satisfy the existing PIN format.
    Incomplete,
    /// A valid candidate does not match its confirmation.
    Mismatch,
    /// The two valid inputs match.
    Ready,
}

/// Checks candidate validity before matching its confirmation, without KDF work.
#[must_use]
pub fn confirmation(pin: &str, confirmed: &str) -> Confirmation {
    if !valid_pin(pin) {
        Confirmation::Incomplete
    } else if bool::from(pin.as_bytes().ct_eq(confirmed.as_bytes())) {
        Confirmation::Ready
    } else {
        Confirmation::Mismatch
    }
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
mod input_tests;
#[cfg(test)]
mod tests;
