//! Practice C6: a configured secret is cleared on drop, redacted in `Debug` and compared only in
//! constant time.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

/// A secret string from the configuration, such as coturn's `static-auth-secret`.
#[derive(Clone, Default)]
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// The secret itself.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// HMAC-SHA1 of `message` keyed by the secret, as coturn's REST credentials need it.
    #[expect(
        clippy::disallowed_types,
        reason = "coturn's use-auth-secret is HMAC-SHA1"
    )]
    pub fn hmac_sha1(&self, message: &[u8]) -> [u8; 20] {
        use hmac::Mac;
        let mut mac = hmac::Hmac::<sha1::Sha1>::new_from_slice(self.0.as_bytes())
            .expect("HMAC takes a key of any length");
        mac.update(message);
        mac.finalize().into_bytes().into()
    }
}

impl From<&str> for Secret {
    fn from(text: &str) -> Self {
        Self(Zeroizing::new(text.to_owned()))
    }
}

/// Shows only whether a secret is set.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_empty() {
            "Secret(unset)"
        } else {
            "Secret(redacted)"
        })
    }
}

/// Constant time in the length of the shorter secret.
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_bytes().ct_eq(other.0.as_bytes()).into()
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| Self(Zeroizing::new(text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_reveals_nothing_and_equality_compares_contents() {
        let secret = Secret::from("hunter2");
        assert_eq!(format!("{secret:?}"), "Secret(redacted)");
        assert_eq!(format!("{:?}", Secret::default()), "Secret(unset)");
        assert_eq!(secret, Secret::from("hunter2"));
        assert_ne!(secret, Secret::from("hunter3"));
        assert_ne!(secret, Secret::from("hunter"));
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(
            serde_json::from_str::<Secret>(&json).unwrap().expose(),
            "hunter2"
        );
    }
}
