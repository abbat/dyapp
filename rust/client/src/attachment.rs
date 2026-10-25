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

/// Largest preview, the same bound as `dyapp_ffi::preview::MAX_BYTES`.
pub const MAX_PREVIEW_BYTES: usize = 24 * 1024;
/// Preview bytes one message carries across all its attachments.
pub const MESSAGE_PREVIEW_BYTES: usize = 64 * 1024;
/// Longest MIME type.
pub const MAX_MIME_BYTES: usize = 64;
// Widest and tallest aspect ratio a layout reserves before download.
const MAX_ASPECT: u64 = 20;

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
    /// Plaintext length in bytes, so a member can decide before fetching.
    pub size: u64,
    /// A JPEG preview of at most [`MAX_PREVIEW_BYTES`] from `make_preview`; empty when none.
    pub preview: Vec<u8>,
    /// The sender's MIME type, printable ASCII of at most [`MAX_MIME_BYTES`]; may be empty.
    pub mime: String,
    /// The sender's claimed width, only to reserve layout space; see [`layout_size`].
    pub width: u32,
    /// The sender's claimed height, like `width`.
    pub height: u32,
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

    /// Refuses a preview or MIME type over its bound or a MIME type that is not printable ASCII.
    pub fn check(&self) -> Result<(), Error> {
        if self.preview.len() > MAX_PREVIEW_BYTES {
            return Err(Error::Attachment("preview too large"));
        }
        if self.mime.len() > MAX_MIME_BYTES || !self.mime.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::Attachment("bad MIME type"));
        }
        Ok(())
    }
}

/// `max_bytes` for each preview of a message with `count` images.
pub fn preview_budget(count: usize) -> usize {
    MAX_PREVIEW_BYTES.min(MESSAGE_PREVIEW_BYTES / count.max(1))
}

/// The layout size for claimed dimensions: the aspect ratio clamped to 1:20..20:1, a zero side
/// counted as 1. The decoded image's real dimensions replace it after download.
pub fn layout_size(width: u32, height: u32) -> (u32, u32) {
    let (width, height) = (u64::from(width.max(1)), u64::from(height.max(1)));
    let width = width.min(height * MAX_ASPECT);
    let height = height.min(width * MAX_ASPECT);
    // Both stay at most the claimed u32 values.
    (width as u32, height as u32)
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
        size: plaintext.len() as u64,
        preview: Vec::new(),
        mime: String::new(),
        width: 0,
        height: 0,
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

/// Hours a node keeps an unreleased attachment by default, `limits.attachment_retention_hours`.
pub const RETENTION_HOURS: u32 = 168;
// The "original available for N more hours" label shows below this.
const WARN_HOURS: u32 = 24;

/// The user's setting for downloading the full file of an attachment without a tap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoDownload {
    /// At once on an unmetered network; a metered or unknown one waits for a tap or a switch.
    #[default]
    ByNetwork,
    /// At once on any network.
    Always,
    /// Only on a tap.
    OnTap,
}

/// Whether to download the full file without a tap. A stranger's never is, in every mode; the
/// platform reports an unknown network as metered. Call again when the network changes.
pub fn auto_download(mode: AutoDownload, metered: bool, from_contact: bool) -> bool {
    from_contact
        && match mode {
            AutoDownload::ByNetwork => !metered,
            AutoDownload::Always => true,
            AutoDownload::OnTap => false,
        }
}

/// Whole hours left of the original sent at `sent_ms`, when under a day remains; `None` before.
/// `Some(0)` means it has expired or is about to.
pub fn hours_left(sent_ms: i64, now_ms: i64) -> Option<u32> {
    const HOUR_MS: i64 = 3_600_000;
    let left = sent_ms
        .saturating_add(i64::from(RETENTION_HOURS) * HOUR_MS)
        .saturating_sub(now_ms);
    (left < i64::from(WARN_HOURS) * HOUR_MS).then(|| (left.max(0) / HOUR_MS) as u32)
}

/// Downloads the blob of `attachment` from the first of `nodes` that holds it and opens it. A
/// recipient never releases it: in MLS every device is a member, so the first release would drop
/// it for the others; it expires after the retention.
pub async fn fetch(nodes: &mut [crate::Node], attachment: &Attachment) -> Result<Vec<u8>, Error> {
    let mut last = Error::Request("no node".into());
    for node in nodes {
        match node.get_media(&attachment.blob_sha256).await {
            Ok(blob) => return open(attachment, &blob),
            Err(error) => last = error,
        }
    }
    Err(last)
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
    fn strangers_and_metered_networks_never_download_without_a_tap() {
        use AutoDownload::*;
        for mode in [ByNetwork, Always, OnTap] {
            for metered in [false, true] {
                assert!(!auto_download(mode, metered, false));
            }
        }
        assert!(auto_download(ByNetwork, false, true));
        assert!(!auto_download(ByNetwork, true, true));
        assert!(auto_download(Always, true, true));
        assert!(!auto_download(OnTap, false, true));
        assert_eq!(AutoDownload::default(), ByNetwork);
    }

    #[test]
    fn hours_left_shows_only_in_the_last_day() {
        let hour = 3_600_000;
        assert_eq!(hours_left(0, 0), None);
        assert_eq!(hours_left(0, 144 * hour), None);
        assert_eq!(hours_left(0, 144 * hour + 1), Some(23));
        assert_eq!(hours_left(0, 167 * hour), Some(1));
        assert_eq!(hours_left(0, 200 * hour), Some(0));
        assert_eq!(hours_left(i64::MAX, i64::MIN), None);
    }

    #[test]
    fn an_album_of_sixteen_previews_fits_one_message() {
        assert_eq!(preview_budget(0), MAX_PREVIEW_BYTES);
        assert_eq!(preview_budget(2), MAX_PREVIEW_BYTES);
        assert_eq!(preview_budget(16) * 16, MESSAGE_PREVIEW_BYTES);
        assert!(preview_budget(1000) * 1000 <= MESSAGE_PREVIEW_BYTES);
    }

    #[test]
    fn layout_clamps_the_claimed_aspect_ratio() {
        assert_eq!(layout_size(4000, 3000), (4000, 3000));
        assert_eq!(layout_size(10_000, 1), (20, 1));
        assert_eq!(layout_size(1, u32::MAX), (1, 20));
        assert_eq!(layout_size(0, 0), (1, 1));
        assert_eq!(layout_size(u32::MAX, u32::MAX), (u32::MAX, u32::MAX));
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
