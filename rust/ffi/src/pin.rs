//! Shared mobile app-lock PIN verifier. The caller protects the record and enforces retries.

use dyapp_client::app_lock::pin;
use zeroize::Zeroizing;

use crate::CoreError;

/// Hash a six-to-64 digit app PIN into a versioned salt and Argon2id tag.
///
/// The caller must protect this record in device-only storage. This function does not enforce
/// retry limits; invoke it off the UI thread because its KDF uses 64 MiB and three passes.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the PIN is not 6–64 ASCII digits, randomness is unavailable,
/// or the Argon2 verifier cannot be initialized or computed.
pub fn hash_app_pin(pin: String) -> Result<Vec<u8>, CoreError> {
    pin::hash(pin).map_err(|error| CoreError::Failed(error.to_string()))
}

/// Check a PIN against a record from [`hash_app_pin`]. Wrong PIN returns `false`; an invalid
/// record or invalid PIN is an error. The caller enforces retry limits, including on errors.
#[uniffi::export]
///
/// # Errors
///
/// Returns an error if the PIN is invalid, the record length or version is unsupported,
/// or the Argon2 verifier cannot be computed. A valid record with a wrong PIN returns `false`.
pub fn verify_app_pin(pin: String, record: Vec<u8>) -> Result<bool, CoreError> {
    let record = Zeroizing::new(record);
    pin::verify(pin, &record).map_err(|error| CoreError::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_wrong_pin_and_random_salt() {
        let first = hash_app_pin("123456".into()).unwrap();
        let second = hash_app_pin("123456".into()).unwrap();
        assert_eq!(first.len(), 49);
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
        assert!(verify_app_pin("123456".into(), vec![2; 49]).is_err());
    }
}
