//! First-frame decoding behind the container guard; never return peer bytes to an OS decoder.

use super::{check_header, dimensions, MAX_DURATION_MS, MAX_FRAMES};
use std::io::Cursor;

#[cfg(test)]
#[path = "decode/tests.rs"]
mod tests;

const DECODER_MEMORY: usize = 16 * 1024 * 1024;

/// A bounded first frame, always RGBA8.
pub struct Frame {
    /// Decoded width.
    pub width: u32,
    /// Decoded height.
    pub height: u32,
    /// Row-major RGBA8 pixels.
    pub pixels: Vec<u8>,
}

/// Validates the container and returns its nontransparent first frame, containing decoder panics.
///
/// # Errors
///
/// Returns an error if container checks, decoding, dimensions, animation limits,
/// or output-size checks fail, the first frame is fully transparent, or the
/// decoder panics.
pub fn first_frame(bytes: &[u8]) -> Result<Frame, &'static str> {
    check_header(bytes)?;
    std::panic::catch_unwind(|| decode(bytes)).map_err(|_| "WebP decoder panic")?
}

fn decode(bytes: &[u8]) -> Result<Frame, &'static str> {
    let mut decoder =
        image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|_| "invalid WebP")?;
    decoder.set_memory_limit(DECODER_MEMORY);
    let (width, height) = decoder.dimensions();
    dimensions(width, height)?;
    if decoder.num_frames() > MAX_FRAMES || decoder.loop_duration() > u64::from(MAX_DURATION_MS) {
        return Err("WebP animation exceeds limit");
    }
    let alpha = decoder.has_alpha();
    let channels = if alpha { 4 } else { 3 };
    let count = width as usize * height as usize;
    let expected = count * channels;
    if decoder.output_buffer_size() != Some(expected) {
        return Err("WebP output size mismatch");
    }
    let mut buffer = vec![0; expected];
    decoder
        .read_image(&mut buffer)
        .map_err(|_| "invalid WebP first frame")?;
    let pixels = if alpha {
        if buffer.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0) {
            return Err("transparent WebP first frame");
        }
        buffer
    } else {
        let mut rgba = Vec::with_capacity(count * 4);
        for rgb in buffer.as_chunks::<3>().0 {
            rgba.extend_from_slice(rgb);
            rgba.push(255);
        }
        rgba
    };
    Ok(Frame {
        width,
        height,
        pixels,
    })
}
