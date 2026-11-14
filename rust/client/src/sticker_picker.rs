//! Bounded picker pixels and manifest-order pages, shared by every client adapter.
use crate::sticker_pack::Pack;
use crate::sticker_webp::decode::{self, Frame};

/// Maximum decoded choices per picker response.
pub const PAGE_SIZE: usize = 8;
const THUMBNAIL_SIDE: u32 = 96;

/// Invalid selector or failed authenticated-content decoding.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Cursor or entry is outside the immutable pack.
    #[error("invalidIndex")]
    InvalidIndex,
    /// Validated content could not produce bounded RGBA.
    #[error("corrupted")]
    Corrupted,
}

/// One manifest entry; no peer encoding or attachment capability.
pub struct Choice {
    /// Manifest position used by the existing send API.
    pub index: usize,
    /// Validated selected grapheme.
    pub emoji: String,
    /// At most 96 pixels on each side, RGBA8.
    pub frame: Frame,
}

/// One bounded immutable-pack page, in manifest order.
pub struct Page {
    /// Authenticated manifest digest.
    pub pack_id: [u8; 32],
    /// Validated display name.
    pub name: String,
    /// Total entries in the immutable pack.
    pub total: usize,
    /// At most eight resized choices.
    pub entries: Vec<Choice>,
    /// Exclusive next offset when more entries remain.
    pub next: Option<usize>,
}

/// Decode and resize one authenticated entry through the same core decoder as chat media.
///
/// # Errors
/// Returns invalidIndex for an absent entry and corrupted for decoding/pixel-layout failure.
pub fn thumbnail(pack: &Pack, index: usize) -> Result<Choice, Error> {
    let entry = pack.stickers().get(index).ok_or(Error::InvalidIndex)?;
    let frame = decode::first_frame(pack.sticker_bytes(index).ok_or(Error::InvalidIndex)?)
        .map_err(|_| Error::Corrupted)?;
    let rgba = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels)
        .ok_or(Error::Corrupted)?;
    let small = image::imageops::thumbnail(&rgba, THUMBNAIL_SIDE, THUMBNAIL_SIDE);
    Ok(Choice {
        index,
        emoji: entry.emoji.clone(),
        frame: Frame {
            width: small.width(),
            height: small.height(),
            pixels: small.into_raw(),
        },
    })
}

/// Read at most eight 96px thumbnails; an offset equal to total returns the empty final page.
///
/// # Errors
/// Returns invalidIndex beyond total, or a selected thumbnail failure without a partial result.
pub fn page(pack: &Pack, after: usize) -> Result<Page, Error> {
    let total = pack.stickers().len();
    if after > total {
        return Err(Error::InvalidIndex);
    }
    let end = (after + PAGE_SIZE).min(total);
    Ok(Page {
        pack_id: *pack.id(),
        name: pack.name().into(),
        total,
        entries: (after..end)
            .map(|index| thumbnail(pack, index))
            .collect::<Result<_, _>>()?,
        next: (end < total).then_some(end),
    })
}

pub mod recents;

#[cfg(test)]
mod tests;
