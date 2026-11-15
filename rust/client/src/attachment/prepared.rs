//! Seal an outgoing prepared chat JPEG under one validation/descriptor policy for every adapter.

use super::Attachment;
use crate::photo::{
    self,
    prepare::{validate_original, Purpose},
};

/// Refusal to seal a prepared chat photo; display codes remain compatible with client adapters.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The original is not JPEG.
    #[error("unsupported")]
    Unsupported,
    /// The original violates the outgoing byte/frame/dimension policy.
    #[error("corrupted")]
    Corrupted,
    /// Optional preview decoding or bounds failed.
    #[error(transparent)]
    Preview(#[from] photo::PhotoError),
    /// Cryptography or final descriptor consistency failed.
    #[error(transparent)]
    Attachment(#[from] crate::Error),
}

/// Validate and seal a prepared JPEG with its optional bounded JPEG preview.
///
/// Incoming originals have a separate JPEG/PNG wire-budget policy in [`super::original`].
///
/// # Errors
/// Returns unsupported/corrupted original, preview validation or cryptographic/descriptor errors.
pub fn seal(
    photo: &[u8],
    preview: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<(Attachment, Vec<u8>), Error> {
    if !photo.starts_with(&[0xff, 0xd8]) {
        return Err(Error::Unsupported);
    }
    validate_original(photo, width, height, Purpose::Chat).map_err(|_| Error::Corrupted)?;
    if !preview.is_empty() {
        photo::check_preview(&preview)?;
    }
    let (mut media, blob) = super::seal(photo)?;
    media.preview = preview;
    media.mime = "image/jpeg".into();
    media.width = width;
    media.height = height;
    media.check()?;
    Ok((media, blob))
}

#[cfg(test)]
mod tests;
