//! Purpose bounds for originals, independent of the platform decoder.

use super::{PrepareError, Purpose, MAX_SOURCE_PIXELS};
use crate::photo::frame_size;

/// Check original JPEG bounds and match the attachment's declared dimensions.
///
/// This checks framing, not entropy decoding; native decoders still reject invalid pixel data.
///
/// # Errors
/// Rejects excessive bytes/pixels, malformed frames and mismatched or empty dimensions.
pub fn validate_original(
    bytes: &[u8],
    width: u32,
    height: u32,
    purpose: Purpose,
) -> Result<(), PrepareError> {
    if bytes.len() > purpose.policy().2 {
        return Err(PrepareError::TooLarge);
    }
    let dimensions = frame_size(bytes).map_err(|_| PrepareError::Unsupported)?;
    if width == 0 || height == 0 || dimensions != (width, height) {
        return Err(PrepareError::Unsupported);
    }
    if u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
        return Err(PrepareError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
