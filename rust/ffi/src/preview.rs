//! Photo previews: a small JPEG the sender makes once, so lists and chats on a metered network
//! never fetch the original. Previews and profile thumbnails come from other users, so
//! [`check_preview`] treats them as hostile input.

use crate::CoreError;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{Rgb, RgbImage};
use jpeg_decoder::{Decoder, PixelFormat};
use std::sync::atomic::{AtomicBool, Ordering};

/// Longest side of a preview, in pixels.
pub const MAX_SIDE: u32 = 320;
/// Largest preview, in bytes.
pub const MAX_BYTES: u32 = 24 * 1024;
const FALLBACK_SIDE: u32 = 160;
const QUALITIES: [u8; 5] = [80, 70, 60, 50, 40];
// A 320 x 320 RGB decode needs 300 KiB; a lying header gets no more than this.
const MAX_ALLOC: u64 = 1024 * 1024;

static METERED: AtomicBool = AtomicBool::new(true);

/// Decoded preview pixels, 4 bytes per pixel, rows top to bottom.
#[derive(uniffi::Record)]
pub struct Rgba {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

fn failed(reason: &str) -> CoreError {
    CoreError::Failed(reason.to_string())
}

/// JPEG preview of `rgba` (the platform decodes the source): long side at most 320 px, quality
/// 80 down to 40 until it fits in `max_bytes` (at most 24 KiB), then the same at 160 px. Errs when
/// even that does not fit. Transparency is flattened onto white; no metadata is written.
#[uniffi::export]
pub fn make_preview(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    max_bytes: u32,
) -> Result<Vec<u8>, CoreError> {
    if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
        return Err(failed("pixels do not match width and height"));
    }
    let limit = max_bytes.min(MAX_BYTES) as usize;
    let flat = RgbImage::from_fn(width, height, |x, y| {
        let at = (y as usize * width as usize + x as usize) * 4;
        let alpha = u32::from(rgba[at + 3]);
        Rgb(std::array::from_fn(|i| {
            ((u32::from(rgba[at + i]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
        }))
    });
    let mut image = flat;
    for side in [MAX_SIDE, FALLBACK_SIDE] {
        let (w, h) = fit(image.width(), image.height(), side);
        if (w, h) != image.dimensions() {
            image = imageops::resize(&image, w, h, FilterType::Triangle);
        }
        for quality in QUALITIES {
            let mut jpeg = Vec::new();
            JpegEncoder::new_with_quality(&mut jpeg, quality)
                .encode_image(&image)
                .map_err(|e| CoreError::Failed(e.to_string()))?;
            if jpeg.len() <= limit {
                return Ok(jpeg);
            }
        }
    }
    Err(failed("no preview fits the byte limit"))
}

fn fit(width: u32, height: u32, side: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= side {
        return (width, height);
    }
    let scale = |v: u32| {
        ((u64::from(v) * u64::from(side) + u64::from(long) / 2) / u64::from(long)).max(1) as u32
    };
    (scale(width), scale(height))
}

/// Decodes a preview or profile thumbnail from another user. Refuses, before decoding, anything
/// over 24 KiB, not a whole JPEG, or with a frame header over 320 x 320; the decoder itself is
/// held to 320 x 320 and 1 MiB, and a decoder panic becomes an error. Call it off the UI thread.
#[uniffi::export]
pub fn check_preview(bytes: Vec<u8>) -> Result<Rgba, CoreError> {
    if bytes.len() > MAX_BYTES as usize {
        return Err(failed("preview over 24 KiB"));
    }
    let (width, height) = frame_size(&bytes)?;
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err(failed("preview over 320 x 320"));
    }
    std::panic::catch_unwind(|| decode(&bytes, width, height))
        .unwrap_or_else(|_| Err(failed("decoder panic")))
}

/// Width and height from the only frame header (SOF) before the first scan.
fn frame_size(bytes: &[u8]) -> Result<(u32, u32), CoreError> {
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
            // Start of scan: entropy-coded data follows.
            0xDA => break,
            // SOF0-SOF15 except DHT (C4), JPG (C8) and DAC (CC).
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

fn decode(bytes: &[u8], width: u32, height: u32) -> Result<Rgba, CoreError> {
    let mut decoder = Decoder::new(bytes);
    decoder.set_max_decoding_buffer_size(MAX_ALLOC as usize);
    let decoded = decoder
        .decode()
        .map_err(|e| CoreError::Failed(e.to_string()))?;
    let info = decoder.info().ok_or_else(|| failed("missing JPEG frame"))?;
    if u32::from(info.width) != width || u32::from(info.height) != height {
        return Err(failed("JPEG frame changed during decoding"));
    }
    let pixels = usize::from(info.width) * usize::from(info.height);
    let mut rgba = Vec::with_capacity(pixels * 4);
    match info.pixel_format {
        PixelFormat::RGB24 if decoded.len() == pixels * 3 => {
            for rgb in decoded.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        PixelFormat::L8 if decoded.len() == pixels => {
            for value in decoded {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        _ => return Err(failed("unsupported preview pixel format")),
    }
    Ok(Rgba {
        width: u32::from(info.width),
        height: u32::from(info.height),
        pixels: rgba,
    })
}

/// The platform reports whether the current network is metered (mobile, roaming, a hotspot or a
/// network marked metered); an unknown or changing network is reported as metered.
#[uniffi::export]
pub fn set_network_metered(metered: bool) {
    METERED.store(metered, Ordering::Relaxed);
}

/// The last report of [`set_network_metered`]; metered until the platform reports. Chat
/// attachments download in full on their own only when this is false.
pub fn network_metered() -> bool {
    METERED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests;
