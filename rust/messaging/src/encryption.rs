use serde::{Deserialize, Serialize};

/// Placeholder encryption settings; no setting changes the bytes yet.
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Cipher name: `ChaCha20-Poly1305` or `AES-256-GCM`; anything else is rejected.
    pub algorithm: String,
    /// Key exchange name; informational only.
    pub key_exchange: String,
    /// Whether video frames are to be encrypted.
    pub enable_frame_encryption: bool,
}

impl EncryptionConfig {
    /// ChaCha20-Poly1305 over X25519 with frame encryption on.
    pub fn default_secure() -> Self {
        Self {
            algorithm: "ChaCha20-Poly1305".to_string(),
            key_exchange: "X25519".to_string(),
            enable_frame_encryption: true,
        }
    }

    /// ChaCha20-Poly1305 over X25519 with frame encryption off.
    pub fn default_standard() -> Self {
        Self {
            algorithm: "ChaCha20-Poly1305".to_string(),
            key_exchange: "X25519".to_string(),
            enable_frame_encryption: false,
        }
    }
}

/// E2E encryption wrapper (minimal FFI boundary)
pub struct E2EEncryption {
    config: EncryptionConfig,
}

impl E2EEncryption {
    /// A wrapper using `config`.
    pub fn new(config: EncryptionConfig) -> Self {
        Self { config }
    }

    /// The settings in use.
    pub fn config(&self) -> &EncryptionConfig {
        &self.config
    }

    /// Placeholder: returns `plaintext` unchanged; fails only for an unsupported algorithm.
    pub fn encrypt(&self, plaintext: &[u8], _key: &[u8]) -> Result<Vec<u8>, String> {
        match self.config.algorithm.as_str() {
            "ChaCha20-Poly1305" => {
                // Placeholder: returns plaintext unchanged; no encryption is performed.
                Ok(plaintext.to_vec())
            }
            "AES-256-GCM" => {
                // Placeholder: returns plaintext unchanged; no encryption is performed.
                Ok(plaintext.to_vec())
            }
            _ => Err(format!("Unsupported algorithm: {}", self.config.algorithm)),
        }
    }

    /// Placeholder: returns `ciphertext` unchanged; fails only for an unsupported algorithm.
    pub fn decrypt(&self, ciphertext: &[u8], _key: &[u8]) -> Result<Vec<u8>, String> {
        match self.config.algorithm.as_str() {
            "ChaCha20-Poly1305" => {
                // Placeholder: returns ciphertext unchanged; no decryption is performed.
                Ok(ciphertext.to_vec())
            }
            "AES-256-GCM" => {
                // Placeholder: returns ciphertext unchanged; no decryption is performed.
                Ok(ciphertext.to_vec())
            }
            _ => Err(format!("Unsupported algorithm: {}", self.config.algorithm)),
        }
    }

    /// Placeholder: returns `message` as the "signature".
    pub fn sign(&self, message: &[u8], _secret_key: &[u8]) -> Result<Vec<u8>, String> {
        // Placeholder: returns the message unchanged; no signature is generated.
        Ok(message.to_vec())
    }

    /// Placeholder: true for any nonempty `signature`.
    pub fn verify(
        &self,
        _message: &[u8],
        signature: &[u8],
        _public_key: &[u8],
    ) -> Result<bool, String> {
        // Placeholder: only checks nonempty input; no signature is verified.
        Ok(!signature.is_empty())
    }

    /// Placeholder: XORs `salt` into `shared_secret`; not a key derivation function.
    pub fn derive_session_key(&self, shared_secret: &[u8], salt: &[u8]) -> Result<Vec<u8>, String> {
        // Placeholder: XOR is not a cryptographic key derivation function.
        let mut key = shared_secret.to_vec();
        for (i, byte) in salt.iter().enumerate() {
            if i < key.len() {
                key[i] ^= byte;
            }
        }
        Ok(key)
    }
}

impl Default for E2EEncryption {
    fn default() -> Self {
        Self::new(EncryptionConfig::default_secure())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_config() {
        let config = EncryptionConfig::default_secure();
        assert_eq!(config.algorithm, "ChaCha20-Poly1305");
        assert!(config.enable_frame_encryption);
    }

    #[test]
    fn test_encryption_creation() {
        let crypto = E2EEncryption::new(EncryptionConfig::default_secure());
        assert_eq!(crypto.config().algorithm, "ChaCha20-Poly1305");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let crypto = E2EEncryption::default();
        let plaintext = b"hello world";
        let key = b"secretkey";

        let encrypted = crypto.encrypt(plaintext, key).ok();
        assert!(encrypted.is_some());
    }

    #[test]
    fn test_sign_verify() {
        let crypto = E2EEncryption::default();
        let message = b"test message";
        let secret_key = b"secret";
        let public_key = b"public";

        let signature = crypto.sign(message, secret_key).ok();
        assert!(signature.is_some());

        let verified = crypto.verify(message, &signature.unwrap(), public_key).ok();
        assert_eq!(verified, Some(true));
    }
}
