//! Shared mobile app-lock PIN verifier. The caller protects the record and enforces retries.

use dyapp_client::app_lock::pin;
use zeroize::Zeroizing;

use crate::CoreError;

/// New-PIN field readiness; this is never an unlock authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AppPinConfirmation {
    /// The candidate does not satisfy the shared PIN format.
    Incomplete,
    /// The confirmation differs from a valid candidate.
    Mismatch,
    /// Both valid fields match.
    Ready,
}

impl From<pin::Confirmation> for AppPinConfirmation {
    fn from(value: pin::Confirmation) -> Self {
        match value {
            pin::Confirmation::Incomplete => Self::Incomplete,
            pin::Confirmation::Mismatch => Self::Mismatch,
            pin::Confirmation::Ready => Self::Ready,
        }
    }
}

/// Existing ASCII/cap input assistance for a transient secure field; never authenticate with it.
#[uniffi::export]
#[must_use]
pub fn app_pin_input(pin: String) -> String {
    pin::normalize_input(pin)
}

/// Cheap format readiness without a KDF, storage access or attempt admission.
#[uniffi::export]
#[must_use]
pub fn app_pin_ready(pin: String) -> bool {
    let pin = Zeroizing::new(pin);
    pin::valid_pin(&pin)
}

/// Shared validity-before-confirmation presentation outcome; never an authorization result.
#[uniffi::export]
#[must_use]
pub fn app_pin_confirmation(pin: String, confirmed: String) -> AppPinConfirmation {
    let pin = Zeroizing::new(pin);
    let confirmed = Zeroizing::new(confirmed);
    pin::confirmation(&pin, &confirmed).into()
}

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
    fn input_and_confirmation_preserve_the_shared_boundary_contract() {
        assert_eq!(app_pin_input("１２a1-23𐐷456".into()), "123456");
        assert_eq!(app_pin_input("1".repeat(65)), "1".repeat(64));
        assert!(!app_pin_ready("12345".into()));
        assert!(app_pin_ready("123456".into()));
        assert!(!app_pin_ready("1".repeat(65)));
        assert_eq!(
            app_pin_confirmation("12345".into(), "12345".into()),
            AppPinConfirmation::Incomplete
        );
        assert_eq!(
            app_pin_confirmation("123456".into(), "123457".into()),
            AppPinConfirmation::Mismatch
        );
        assert_eq!(
            app_pin_confirmation("123456".into(), "123456".into()),
            AppPinConfirmation::Ready
        );
        assert!(hash_app_pin("1a23456".into()).is_err());
    }

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
