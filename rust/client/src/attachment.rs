//! Chat attachments: each one is encrypted under a fresh random 256-bit key, AES-256-GCM per
//! [`CHUNK_BYTES`] chunk with the chunk index as the nonce, so a node holds only ciphertext. The
//! [`Attachment`] goes inside the MLS message; only its members can fetch, check and open the blob.

use crate::Error;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use dyapp_identity::sha256;

/// Plaintext bytes per chunk; every chunk but the last is full.
pub const CHUNK_BYTES: usize = 64 * 1024;
const TAG_BYTES: usize = 16;
// Additional data of the last chunk and of the others: a blob cut at a chunk boundary fails.
const LAST: &[u8] = &[1];
const MORE: &[u8] = &[0];

/// What a member needs to fetch, check and open an attachment; sent inside the MLS message.
#[derive(Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The single-use AES-256-GCM key.
    pub key: [u8; 32],
    /// SHA-256 of the plaintext.
    pub plaintext_sha256: [u8; 32],
    /// SHA-256 of the blob: the media hash it is stored and fetched under.
    pub blob_sha256: [u8; 32],
    /// 32 random bytes; the node holds only their SHA-256, [`Attachment::release_hash`].
    pub release_secret: [u8; 32],
}

impl std::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("blob_sha256", &self.blob_sha256)
            .finish_non_exhaustive()
    }
}

impl Attachment {
    /// The hash the sender attaches the blob under; the release secret opens it.
    pub fn release_hash(&self) -> [u8; 32] {
        sha256(&self.release_secret)
    }
}

/// Encrypts `plaintext` under a fresh key; returns its [`Attachment`] and the blob to upload.
pub fn seal(plaintext: &[u8]) -> Result<(Attachment, Vec<u8>), Error> {
    let key = random()?;
    let cipher = Aes256Gcm::new(&key.into());
    let count = plaintext.len().div_ceil(CHUNK_BYTES).max(1);
    let mut blob = Vec::with_capacity(plaintext.len() + count * TAG_BYTES);
    for index in 0..count {
        let chunk = plaintext
            .get(index * CHUNK_BYTES..)
            .map_or(&[][..], |rest| &rest[..rest.len().min(CHUNK_BYTES)]);
        let aad = if index + 1 == count { LAST } else { MORE };
        let sealed = cipher
            .encrypt(&nonce(index), Payload { msg: chunk, aad })
            .map_err(|_| Error::Attachment("encryption failed"))?;
        blob.extend_from_slice(&sealed);
    }
    let attachment = Attachment {
        key,
        plaintext_sha256: sha256(plaintext),
        blob_sha256: sha256(&blob),
        release_secret: random()?,
    };
    Ok((attachment, blob))
}

/// Checks `blob` against `attachment` and decrypts it. Fails on any changed, reordered or
/// missing byte, and on a wrong key.
pub fn open(attachment: &Attachment, blob: &[u8]) -> Result<Vec<u8>, Error> {
    if sha256(blob) != attachment.blob_sha256 {
        return Err(Error::Attachment("blob hash mismatch"));
    }
    let cipher = Aes256Gcm::new(&attachment.key.into());
    let chunks: Vec<&[u8]> = blob.chunks(CHUNK_BYTES + TAG_BYTES).collect();
    if chunks.is_empty() {
        return Err(Error::Attachment("empty blob"));
    }
    let last = chunks.len() - 1;
    let mut plaintext = Vec::with_capacity(blob.len());
    for (index, chunk) in chunks.iter().enumerate() {
        let aad = if index == last { LAST } else { MORE };
        let opened = cipher
            .decrypt(&nonce(index), Payload { msg: chunk, aad })
            .map_err(|_| Error::Attachment("chunk does not decrypt"))?;
        plaintext.extend_from_slice(&opened);
    }
    if sha256(&plaintext) != attachment.plaintext_sha256 {
        return Err(Error::Attachment("plaintext hash mismatch"));
    }
    Ok(plaintext)
}

// The 96-bit GCM nonce: the chunk index, big-endian. Safe because every key is used once.
fn nonce(index: usize) -> Nonce<aes_gcm::aead::consts::U12> {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&(index as u64).to_be_bytes());
    nonce.into()
}

fn random() -> Result<[u8; 32], Error> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Attachment("no OS randomness"))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        (0..CHUNK_BYTES * 2 + 100).map(|i| i as u8).collect()
    }

    // A blob altered after sealing, with its hash updated so the chunk check is what fails.
    fn opens_altered(change: impl FnOnce(&mut Vec<u8>)) -> Result<Vec<u8>, Error> {
        let (mut attachment, mut blob) = seal(&sample()).unwrap();
        change(&mut blob);
        attachment.blob_sha256 = sha256(&blob);
        open(&attachment, &blob)
    }

    #[test]
    fn round_trips_any_size() {
        for size in [0, 1, CHUNK_BYTES, CHUNK_BYTES + 1, CHUNK_BYTES * 2 + 100] {
            let plaintext = vec![9; size];
            let (attachment, blob) = seal(&plaintext).unwrap();
            assert_eq!(
                blob.len(),
                size + size.div_ceil(CHUNK_BYTES).max(1) * TAG_BYTES
            );
            assert_eq!(open(&attachment, &blob).unwrap(), plaintext, "{size}");
        }
    }

    #[test]
    fn tampered_reordered_truncated_and_wrong_key_fail() {
        let chunk = CHUNK_BYTES + TAG_BYTES;
        assert!(opens_altered(|blob| blob[10] ^= 1).is_err());
        assert!(opens_altered(|blob| blob[..2 * chunk].rotate_left(chunk)).is_err());
        assert!(opens_altered(|blob| blob.truncate(2 * chunk)).is_err());
        assert!(opens_altered(|blob| blob.truncate(blob.len() - 1)).is_err());
        assert!(opens_altered(Vec::clear).is_err());

        let (mut attachment, blob) = seal(&sample()).unwrap();
        attachment.key[0] ^= 1;
        assert!(open(&attachment, &blob).is_err());
        let (attachment, mut blob) = seal(&sample()).unwrap();
        blob[0] ^= 1;
        assert!(open(&attachment, &blob).is_err(), "blob hash is checked");
    }

    #[test]
    fn every_attachment_has_its_own_key_and_secret() {
        let (first, first_blob) = seal(b"same").unwrap();
        let (second, second_blob) = seal(b"same").unwrap();
        assert_ne!(first.key, second.key);
        assert_ne!(first.release_secret, second.release_secret);
        assert_ne!(first_blob, second_blob);
        assert_eq!(first.release_hash(), sha256(&first.release_secret));
        assert!(!format!("{first:?}").contains(&format!("{:?}", first.key)));
    }
}
