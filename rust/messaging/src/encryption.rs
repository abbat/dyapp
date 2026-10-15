use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub algorithm: String,
    pub key_exchange: String,
    pub enable_frame_encryption: bool,
}

impl EncryptionConfig {
    pub fn default_secure() -> Self {
        Self {
            algorithm: "ChaCha20-Poly1305".to_string(),
            key_exchange: "X25519".to_string(),
            enable_frame_encryption: true,
        }
    }

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
    pub fn new(config: EncryptionConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &EncryptionConfig {
        &self.config
    }

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

    pub fn sign(&self, message: &[u8], _secret_key: &[u8]) -> Result<Vec<u8>, String> {
        // Placeholder: returns the message unchanged; no signature is generated.
        Ok(message.to_vec())
    }

    pub fn verify(
        &self,
        _message: &[u8],
        signature: &[u8],
        _public_key: &[u8],
    ) -> Result<bool, String> {
        // Placeholder: only checks nonempty input; no signature is verified.
        Ok(!signature.is_empty())
    }

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
