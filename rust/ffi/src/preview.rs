//! Common `UniFFI` adapter to the shared core photo codecs.

use crate::CoreError;
use dyapp_client::photo;
#[cfg(test)]
use photo::MAX_BYTES;
pub(crate) use photo::MAX_SIDE;
use std::sync::atomic::{AtomicBool, Ordering};

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

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::Failed(error.to_string())
}

/// Encode a metadata-free, white-flattened preview through the shared photo core.
///
/// # Errors
/// Returns an error for invalid dimensions, encoding failures or an impossible byte limit.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn make_preview(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    max_bytes: u32,
) -> Result<Vec<u8>, CoreError> {
    photo::make_preview(&rgba, width, height, max_bytes).map_err(failed)
}

/// Decode a hostile peer preview with the shared core byte, frame and allocation bounds.
///
/// # Errors
/// Returns an error for malformed or oversized JPEGs, decoder failures or inconsistent pixels.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI lifts owned arguments across the native language boundary"
)]
pub fn check_preview(bytes: Vec<u8>) -> Result<Rgba, CoreError> {
    photo::check_preview(&bytes)
        .map(|decoded| Rgba {
            width: decoded.width,
            height: decoded.height,
            pixels: decoded.pixels,
        })
        .map_err(failed)
}

/// Inspect a JPEG frame through the shared core without decoding pixels.
pub(crate) fn frame_size(bytes: &[u8]) -> Result<(u32, u32), CoreError> {
    photo::frame_size(bytes).map_err(failed)
}

/// Report metered or unknown network state; platform adapters own this report.
#[uniffi::export]
pub fn set_network_metered(metered: bool) {
    METERED.store(metered, Ordering::Relaxed);
}

/// Last platform report; defaults to metered, so automatic downloads fail closed.
pub fn network_metered() -> bool {
    METERED.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests;
