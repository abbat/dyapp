//! Shared preview encoding and bounded decoding of hostile peer JPEGs.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use jpeg_decoder::{Decoder, PixelFormat};
use std::borrow::Cow;

pub mod prepare;

/// Longest side of a preview, in pixels.
pub const MAX_SIDE: u32 = 320;
/// Largest preview, in bytes.
pub const MAX_BYTES: u32 = 24 * 1024;
const FALLBACK_SIDE: u32 = 160;
const QUALITIES: [u8; 5] = [80, 70, 60, 50, 40];
// A bounded RGB decode needs 300 KiB; a lying header gets no more than this.
const MAX_ALLOC: usize = 1024 * 1024;

/// Why an image failed validation, encoding or bounded decoding.
#[derive(Debug, thiserror::Error)]
pub enum PhotoError {
    /// Invalid pixels or JPEG structure; the reason is safe to show to the caller.
    #[error("{0}")]
    Invalid(&'static str),
    /// Encoder or decoder failure.
    #[error("{0}")]
    Codec(String),
    /// No preview candidate fits the requested byte limit.
    #[error("no preview fits the byte limit")]
    ByteLimit,
}

/// Decoded preview pixels, four bytes per pixel, rows top to bottom.
#[derive(Debug, PartialEq, Eq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Exactly `width * height * 4` RGBA bytes.
    pub pixels: Vec<u8>,
}

fn failed(reason: &'static str) -> PhotoError {
    PhotoError::Invalid(reason)
}

/// Encode a metadata-free preview, flattening transparency onto white.
///
/// Tries quality 80 through 40 at 320 px, then 160 px, within at most 24 KiB.
///
/// # Errors
/// Rejects mismatched or overflowing dimensions, encoder failures, or a byte limit no preview fits.
pub fn make_preview(
    rgba: &[u8],
    width: u32,
    height: u32,
    max_bytes: u32,
) -> Result<Vec<u8>, PhotoError> {
    preview_rgb(&flatten(rgba, width, height)?, max_bytes)
}

fn flatten(rgba: &[u8], width: u32, height: u32) -> Result<RgbImage, PhotoError> {
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| failed("pixels do not match width and height"))?;
    if width == 0 || height == 0 || u64::try_from(rgba.len()).ok() != Some(expected) {
        return Err(failed("pixels do not match width and height"));
    }
    Ok(RgbImage::from_fn(width, height, |x, y| {
        let at = (y as usize * width as usize + x as usize) * 4;
        let alpha = u32::from(rgba[at + 3]);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Rounded blending of u8 colors stays within 0..=255"
        )]
        Rgb(std::array::from_fn(|i| {
            ((u32::from(rgba[at + i]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        }))
    }))
}

fn preview_rgb(flat: &RgbImage, max_bytes: u32) -> Result<Vec<u8>, PhotoError> {
    let limit = max_bytes.min(MAX_BYTES) as usize;
    let mut image = Cow::Borrowed(flat);
    for side in [MAX_SIDE, FALLBACK_SIDE] {
        let (w, h) = fit(image.width(), image.height(), side);
        if (w, h) != image.dimensions() {
            image = Cow::Owned(imageops::resize(image.as_ref(), w, h, FilterType::Triangle));
        }
        for quality in QUALITIES {
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, quality)
                .encode_image(image.as_ref())
                .map_err(|error| PhotoError::Codec(error.to_string()))?;
            if jpeg.len() <= limit {
                return Ok(jpeg);
            }
        }
    }
    Err(PhotoError::ByteLimit)
}

fn fit(width: u32, height: u32, side: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= side {
        return (width, height);
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "v <= long and side < long, so dimensions stay in 1..=side"
    )]
    let scale = |v: u32| {
        ((u64::from(v) * u64::from(side) + u64::from(long) / 2) / u64::from(long)).max(1) as u32
    };
    (scale(width), scale(height))
}

/// Decode a peer preview with 24 KiB input, 320 px sides and a 1 MiB decoder allocation limit.
///
/// # Errors
/// Rejects malformed, oversized or unsupported JPEGs, decoder failures/panics and inconsistent pixels.
pub fn check_preview(bytes: &[u8]) -> Result<Rgba, PhotoError> {
    if bytes.len() > MAX_BYTES as usize {
        return Err(failed("preview over 24 KiB"));
    }
    let (width, height) = frame_size(bytes)?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(failed("preview over 320 x 320"));
    }
    std::panic::catch_unwind(|| decode(bytes, width, height))
        .unwrap_or_else(|_| Err(failed("decoder panic")))
}

/// Inspect the only JPEG frame header before its first scan, without decoding pixels.
///
/// This does not impose preview size limits; callers decide the permitted input/frame bounds.
///
/// # Errors
/// Rejects incomplete JPEGs, malformed markers, missing frame headers and repeated frame headers.
pub fn frame_size(bytes: &[u8]) -> Result<(u32, u32), PhotoError> {
    if !bytes.starts_with(&[0xFF, 0xD8]) || !bytes.ends_with(&[0xFF, 0xD9]) {
        return Err(failed("not a whole JPEG"));
    }
    let mut at = 2;
    let mut size = None;
    loop {
        let segment = bytes
            .get(at..at + 4)
            .ok_or_else(|| failed("truncated JPEG header"))?;
        let [0xFF, marker, len_hi, len_lo] = *segment else {
            return Err(failed("bad JPEG marker"));
        };
        match marker {
            0xDA => break,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let sof = bytes
                    .get(at + 5..at + 9)
                    .ok_or_else(|| failed("truncated SOF"))?;
                let height = u32::from(u16::from_be_bytes([sof[0], sof[1]]));
                let width = u32::from(u16::from_be_bytes([sof[2], sof[3]]));
                if size.replace((width, height)).is_some() {
                    return Err(failed("repeated SOF"));
                }
            }
            _ => {}
        }
        at += 2 + usize::from(u16::from_be_bytes([len_hi, len_lo]));
    }
    size.ok_or_else(|| failed("no SOF before the scan"))
}

fn decode(bytes: &[u8], width: u32, height: u32) -> Result<Rgba, PhotoError> {
    let mut decoder = Decoder::new(bytes);
    decoder.set_max_decoding_buffer_size(MAX_ALLOC);
    let buffer = decoder
        .decode()
        .map_err(|error| PhotoError::Codec(error.to_string()))?;
    let info = decoder.info().ok_or_else(|| failed("missing JPEG frame"))?;
    if u32::from(info.width) != width || u32::from(info.height) != height {
        return Err(failed("JPEG frame changed during decoding"));
    }
    let pixels = usize::from(info.width) * usize::from(info.height);
    Ok(Rgba {
        width: u32::from(info.width),
        height: u32::from(info.height),
        pixels: rgba_pixels(&buffer, info.pixel_format, pixels)?,
    })
}

fn rgba_pixels(decoded: &[u8], format: PixelFormat, pixels: usize) -> Result<Vec<u8>, PhotoError> {
    let mut rgba = Vec::with_capacity(pixels * 4);
    match format {
        PixelFormat::RGB24 if decoded.len() == pixels * 3 => {
            for rgb in decoded.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        PixelFormat::L8 if decoded.len() == pixels => {
            for &value in decoded {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        _ => return Err(failed("unsupported preview pixel format")),
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests;
