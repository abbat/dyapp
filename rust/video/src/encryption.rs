use dyapp_messaging::encryption::{E2EEncryption, EncryptionConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameHeader {
    pub frame_id: u64,
    pub timestamp: u64,
    pub width: u16,
    pub height: u16,
    pub nonce: Vec<u8>,
    pub tag: Vec<u8>,
}

#[derive(Debug)]
pub struct EncryptedFrame {
    pub header: FrameHeader,
    pub ciphertext: Vec<u8>,
}

pub struct FrameEncryption {
    encryption: E2EEncryption,
    frame_counter: u64,
}

impl FrameEncryption {
    pub fn new() -> Self {
        Self {
            encryption: E2EEncryption::default(),
            frame_counter: 0,
        }
    }

    pub fn with_config(config: EncryptionConfig) -> Self {
        Self {
            encryption: E2EEncryption::new(config),
            frame_counter: 0,
        }
    }

    pub fn encrypt_frame(
        &mut self,
        frame_data: &[u8],
        key: &[u8],
        width: u16,
        height: u16,
    ) -> Result<EncryptedFrame, String> {
        let ciphertext = self.encryption.encrypt(frame_data, key)?;

        let nonce = (0..12).map(|_| rand::random::<u8>()).collect::<Vec<_>>();
        let tag = (0..16).map(|_| rand::random::<u8>()).collect::<Vec<_>>();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        self.frame_counter += 1;

        Ok(EncryptedFrame {
            header: FrameHeader {
                frame_id: self.frame_counter,
                timestamp,
                width,
                height,
                nonce,
                tag,
            },
            ciphertext,
        })
    }

    pub fn decrypt_frame(&self, encrypted: &EncryptedFrame, key: &[u8]) -> Result<Vec<u8>, String> {
        self.encryption.decrypt(&encrypted.ciphertext, key)
    }

    pub fn is_enabled(&self) -> bool {
        self.encryption.config().enable_frame_encryption
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_counter
    }

    pub fn reset_counter(&mut self) {
        self.frame_counter = 0;
    }
}

impl Default for FrameEncryption {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encryption_creation() {
        let encryption = FrameEncryption::new();
        assert!(encryption.is_enabled());
        assert_eq!(encryption.frame_count(), 0);
    }

    #[test]
    fn test_frame_counter_increment() {
        let mut encryption = FrameEncryption::new();
        let frame_data = b"test frame data";
        let key = b"secretkey";

        let _ = encryption.encrypt_frame(frame_data, key, 1920, 1080);
        assert_eq!(encryption.frame_count(), 1);

        let _ = encryption.encrypt_frame(frame_data, key, 1920, 1080);
        assert_eq!(encryption.frame_count(), 2);
    }

    #[test]
    fn test_frame_header_structure() {
        let mut encryption = FrameEncryption::new();
        let frame_data = b"test frame";
        let key = b"key";

        let encrypted = encryption.encrypt_frame(frame_data, key, 640, 480).ok();
        assert!(encrypted.is_some());

        let encrypted = encrypted.unwrap();
        assert_eq!(encrypted.header.width, 640);
        assert_eq!(encrypted.header.height, 480);
        assert_eq!(encrypted.header.frame_id, 1);
    }

    #[test]
    fn test_frame_counter_reset() {
        let mut encryption = FrameEncryption::new();
        let frame_data = b"test";
        let key = b"key";

        let _ = encryption.encrypt_frame(frame_data, key, 640, 480);
        assert_eq!(encryption.frame_count(), 1);

        encryption.reset_counter();
        assert_eq!(encryption.frame_count(), 0);
    }
}
