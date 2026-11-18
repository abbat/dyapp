//! Incoming chat photos follow the attachment wire budget, not the composer output budget.

use super::{LookupError, MAX_BLOB_BYTES};
use crate::attachment::sealed_size;
use crate::photo::{frame_size, prepare::MAX_SOURCE_PIXELS};
mod png;

/// Check JPEG/PNG headers and every specified dimension before platform decoding.
///
/// Zero descriptor dimensions mean unspecified. This validates framing/bounds, not pixel data;
/// platform decoders must still reject corrupt pixels. The sealed node blob fits at most 6 MiB.
///
/// # Errors
/// Returns `corrupted` for unsupported headers, oversized bytes/pixels or mismatched dimensions.
pub fn validate(bytes: &[u8], width: u32, height: u32) -> Result<(), LookupError> {
    if sealed_size(bytes.len()).is_none_or(|size| size > MAX_BLOB_BYTES) {
        return Err(LookupError::Corrupted);
    }
    let (actual_width, actual_height) = if bytes.starts_with(&[0xff, 0xd8]) {
        frame_size(bytes).map_err(|_| LookupError::Corrupted)?
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png::dimensions(bytes)?
    } else {
        return Err(LookupError::Corrupted);
    };
    if actual_width == 0
        || actual_height == 0
        || u64::from(actual_width)
            .checked_mul(u64::from(actual_height))
            .is_none_or(|pixels| pixels > MAX_SOURCE_PIXELS)
        || (width != 0 && width != actual_width)
        || (height != 0 && height != actual_height)
    {
        return Err(LookupError::Corrupted);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
