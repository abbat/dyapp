//! Shared mobile app-lock PIN verifier. The caller protects the record and enforces retries.

use argon2::{Algorithm, Argon2, Params, Version};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::CoreError;

const FORMAT: u8 = 1;
const SALT_LEN: usize = 16;
const TAG_LEN: usize = 32;
const RECORD_LEN: usize = 1 + SALT_LEN + TAG_LEN;

fn valid_pin(pin: &str) -> bool {
    (6..=64).contains(&pin.len()) && pin.bytes().all(|byte| byte.is_ascii_digit())
}

fn tag(pin: &str, salt: &[u8]) -> Result<[u8; TAG_LEN], CoreError> {
    let params = Params::new(64 * 1024, 3, 4, Some(TAG_LEN))
        .map_err(|_| CoreError::Failed("PIN verifier unavailable".into()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0; TAG_LEN];
    argon
        .hash_password_into(pin.as_bytes(), salt, &mut output)
        .map_err(|_| CoreError::Failed("PIN verifier unavailable".into()))?;
    Ok(output)
}

/// Hash a six-to-64 digit app PIN into a versioned salt and Argon2id tag.
///
/// The caller must protect this record in device-only storage. This function does not enforce
/// retry limits; invoke it off the UI thread because its KDF uses 64 MiB and three passes.
#[uniffi::export]
pub fn hash_app_pin(pin: String) -> Result<Vec<u8>, CoreError> {
    let pin = Zeroizing::new(pin);
    if !valid_pin(&pin) {
        return Err(CoreError::Failed(
            "PIN must contain 6 to 64 decimal digits".into(),
        ));
    }
    let mut salt = [0; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|_| CoreError::Failed("PIN verifier unavailable".into()))?;
    let tag = Zeroizing::new(tag(&pin, &salt)?);
    let mut record = Vec::with_capacity(RECORD_LEN);
    record.push(FORMAT);
    record.extend_from_slice(&salt);
    record.extend_from_slice(&tag[..]);
    Ok(record)
}

/// Check a PIN against a record from [`hash_app_pin`]. Wrong PIN returns `false`; an invalid
/// record or invalid PIN is an error. The caller enforces retry limits, including on errors.
#[uniffi::export]
pub fn verify_app_pin(pin: String, record: Vec<u8>) -> Result<bool, CoreError> {
    let pin = Zeroizing::new(pin);
    if !valid_pin(&pin) {
        return Err(CoreError::Failed(
            "PIN must contain 6 to 64 decimal digits".into(),
        ));
    }
    if record.len() != RECORD_LEN || record[0] != FORMAT {
        return Err(CoreError::Failed("Invalid PIN verifier record".into()));
    }
    let computed = Zeroizing::new(tag(&pin, &record[1..1 + SALT_LEN])?);
    Ok(bool::from(computed[..].ct_eq(&record[1 + SALT_LEN..])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_pin_and_random_salt() {
        let first = hash_app_pin("123456".into()).unwrap();
        let second = hash_app_pin("123456".into()).unwrap();
        assert_eq!(first.len(), RECORD_LEN);
        assert_ne!(first, second);
        assert!(verify_app_pin("123456".into(), first.clone()).unwrap());
        assert!(!verify_app_pin("123457".into(), first).unwrap());
    }

    #[test]
    fn rejects_bad_input_and_record_without_echoing_pin() {
        let err = hash_app_pin("12abc6".into()).unwrap_err().to_string();
        assert!(!err.contains("12abc6"));
        assert!(hash_app_pin("12345".into()).is_err());
        assert!(verify_app_pin("123456".into(), vec![]).is_err());
        assert!(verify_app_pin("123456".into(), vec![2; RECORD_LEN]).is_err());
    }
}
