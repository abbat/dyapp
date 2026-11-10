//! Purpose-specific full-photo preparation; platforms execute the core's decode plan.

use super::{fit, flatten, preview_rgb, PhotoError, MAX_BYTES};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::RgbImage;
use std::borrow::Cow;

/// Largest picked JPEG or PNG file, before decoding.
pub const MAX_PICKED_BYTES: u64 = 64 * 1024 * 1024;
/// Largest source frame, checked before decoding pixels.
pub const MAX_SOURCE_PIXELS: u64 = 64_000_000;

/// Which published image budget and fitting policy to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Public profile original: at most 1 MiB.
    Profile,
    /// Chat original: at most 6,000,000 bytes before attachment encryption.
    Chat,
}

impl Purpose {
    fn policy(self) -> (&'static [u32], &'static [u8], usize) {
        match self {
            Self::Profile => (&[2048, 1600, 1280], &[85, 70, 55], 1024 * 1024),
            Self::Chat => (&[2560, 1920, 1280], &[85], 6_000_000),
        }
    }
}

/// Platform decoder target, maintaining aspect ratio without upscaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodePlan {
    /// Upright target width.
    pub width: u32,
    /// Upright target height.
    pub height: u32,
}

/// Safe user-facing preparation failure category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PrepareError {
    /// Source or decoded input exceeds its applicable budget, or no encoded candidate fits.
    #[error("tooLarge")]
    TooLarge,
    /// Unsupported format, malformed pixels or an encoder failure.
    #[error("unsupported")]
    Unsupported,
}

/// Validate a picked header and return its maximum decoded size.
///
/// # Errors
/// Rejects unsupported MIME types, empty dimensions and oversized bytes or source frames.
pub fn decode_plan(
    mime: &str,
    width: u32,
    height: u32,
    bytes: u64,
    purpose: Purpose,
) -> Result<DecodePlan, PrepareError> {
    if !matches!(mime, "image/jpeg" | "image/png") || width == 0 || height == 0 {
        return Err(PrepareError::Unsupported);
    }
    if bytes > MAX_PICKED_BYTES || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
        return Err(PrepareError::TooLarge);
    }
    let (width, height) = fit(width, height, purpose.policy().0[0]);
    Ok(DecodePlan { width, height })
}

/// Metadata-free original and shared preview, with the original's actual dimensions.
#[derive(Debug)]
pub struct PreparedPhoto {
    /// Encoded original within the selected purpose's byte budget.
    pub full: Vec<u8>,
    /// Bounded shared preview.
    pub thumbnail: Vec<u8>,
    /// Actual original width after fitting.
    pub width: u32,
    /// Actual original height after fitting.
    pub height: u32,
}

/// Prepare upright straight RGBA already decoded within the core's purpose target.
///
/// # Errors
/// Rejects oversized decoded dimensions, mismatched pixels, encoder failures or candidates that do not fit.
pub fn prepare_photo(
    rgba: &[u8],
    width: u32,
    height: u32,
    purpose: Purpose,
) -> Result<PreparedPhoto, PrepareError> {
    let (sides, qualities, limit) = purpose.policy();
    if width.max(height) > sides[0] {
        return Err(PrepareError::TooLarge);
    }
    let flat = flatten(rgba, width, height).map_err(|_| PrepareError::Unsupported)?;
    let (full, width, height) = fitted(&flat, sides, qualities, limit)?;
    let thumbnail = preview_rgb(&flat, MAX_BYTES).map_err(|error| match error {
        PhotoError::ByteLimit => PrepareError::TooLarge,
        _ => PrepareError::Unsupported,
    })?;
    Ok(PreparedPhoto {
        full,
        thumbnail,
        width,
        height,
    })
}

fn jpeg(image: &RgbImage, quality: u8) -> Result<Vec<u8>, PrepareError> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, quality)
        .encode_image(image)
        .map_err(|_| PrepareError::Unsupported)?;
    Ok(bytes)
}

fn fitted(
    flat: &RgbImage,
    sides: &[u32],
    qualities: &[u8],
    limit: usize,
) -> Result<(Vec<u8>, u32, u32), PrepareError> {
    for &side in sides {
        let (width, height) = fit(flat.width(), flat.height(), side);
        let image = if (width, height) == flat.dimensions() {
            Cow::Borrowed(flat)
        } else {
            Cow::Owned(imageops::resize(flat, width, height, FilterType::Triangle))
        };
        for &quality in qualities {
            let bytes = jpeg(image.as_ref(), quality)?;
            if bytes.len() <= limit {
                return Ok((bytes, width, height));
            }
        }
    }
    Err(PrepareError::TooLarge)
}

#[cfg(test)]
mod tests;
