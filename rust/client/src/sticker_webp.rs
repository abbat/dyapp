//! WebP container checks before any decoder allocation. This is not a bitstream validator.
//! Layout follows <https://developers.google.com/speed/webp/docs/riff_container>.

#[cfg(test)]
#[path = "sticker_webp/tests.rs"]
mod tests;

/// First-frame RGBA decoding after the container guard.
#[path = "sticker_webp/decode.rs"]
pub mod decode;

const MAX_BYTES: usize = 256 * 1024;
const MAX_SIDE: u32 = 512;
const MAX_FRAMES: u32 = 90;
const MAX_DURATION_MS: u32 = 3_000;

fn little(bytes: &[u8]) -> u32 {
    bytes.iter().enumerate().fold(0, |value, (at, byte)| {
        value | (u32::from(*byte) << (at * 8))
    })
}

fn dimensions(width: u32, height: u32) -> Result<(), &'static str> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err("sticker dimensions exceed limit");
    }
    Ok(())
}

fn frame_chunks(mut bytes: &[u8], expected: (u32, u32)) -> Result<(), &'static str> {
    let mut images = 0;
    let mut alpha = false;
    while !bytes.is_empty() {
        let header = bytes.get(..8).ok_or("truncated frame chunk")?;
        let size = little(&header[4..8]) as usize;
        let end = 8usize.checked_add(size).ok_or("frame size overflow")?;
        let data = bytes.get(8..end).ok_or("truncated frame payload")?;
        match &header[..4] {
            b"VP8L" => {
                if data.len() < 5 || data[0] != 0x2f {
                    return Err("invalid frame VP8L");
                }
                let bits = little(&data[1..5]);
                let size = ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1);
                dimensions(size.0, size.1)?;
                if size != expected || bits >> 29 != 0 || alpha {
                    return Err("frame VP8L header mismatch");
                }
                images += 1;
            }
            b"VP8 " => {
                if data.len() < 10 || &data[3..6] != b"\x9d\x01\x2a" {
                    return Err("invalid frame VP8");
                }
                let size = (little(&data[6..8]) & 0x3fff, little(&data[8..10]) & 0x3fff);
                dimensions(size.0, size.1)?;
                if size != expected {
                    return Err("frame VP8 header mismatch");
                }
                images += 1;
            }
            b"ALPH" if images == 0 && !alpha => alpha = true,
            _ => return Err("unsupported animation frame chunk"),
        }
        let padded = end.checked_add(size % 2).ok_or("frame padding overflow")?;
        if padded > bytes.len() || (!size.is_multiple_of(2) && bytes[end] != 0) {
            return Err("invalid frame padding");
        }
        bytes = &bytes[padded..];
    }
    if images != 1 {
        return Err("frame needs exactly one image");
    }
    Ok(())
}

#[derive(Default)]
struct HeaderState {
    frames: u32,
    duration: u32,
    image: bool,
    canvas: Option<(u32, u32)>,
    animation: bool,
    animated_header: bool,
}

impl HeaderState {
    fn check_vp8x(&mut self, data: &[u8], offset: usize) -> Result<(), &'static str> {
        if data.len() != 10 || offset != 12 {
            return Err("invalid VP8X header");
        }
        if data[0] & 0x0c != 0 {
            return Err("WebP metadata flags are forbidden");
        }
        if data[0] & 0xc1 != 0 || data[1..4] != [0, 0, 0] {
            return Err("invalid VP8X reserved bits");
        }
        let size = (little(&data[4..7]) + 1, little(&data[7..10]) + 1);
        dimensions(size.0, size.1)?;
        self.canvas = Some(size);
        self.animated_header = data[0] & 2 != 0;
        Ok(())
    }

    fn check_vp8(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 10 || &data[3..6] != b"\x9d\x01\x2a" {
            return Err("invalid VP8 header");
        }
        let size = (little(&data[6..8]) & 0x3fff, little(&data[8..10]) & 0x3fff);
        dimensions(size.0, size.1)?;
        if self.canvas.is_some_and(|canvas| canvas != size) {
            return Err("static VP8 header mismatch");
        }
        self.image = true;
        Ok(())
    }

    fn check_vp8l(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 5 || data[0] != 0x2f {
            return Err("invalid VP8L header");
        }
        let bits = little(&data[1..5]);
        let size = ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1);
        dimensions(size.0, size.1)?;
        if bits >> 29 != 0 || self.canvas.is_some_and(|canvas| canvas != size) {
            return Err("static VP8L header mismatch");
        }
        self.image = true;
        Ok(())
    }

    fn check_anim(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if !self.animated_header || self.animation || data.len() != 6 {
            return Err("invalid animation header");
        }
        self.animation = true;
        Ok(())
    }

    fn check_anmf(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 16 || !self.animation {
            return Err("invalid animation frame");
        }
        let size = (little(&data[6..9]) + 1, little(&data[9..12]) + 1);
        dimensions(size.0, size.1)?;
        let (width, height) = self.canvas.ok_or("frame has no canvas")?;
        let right = little(&data[..3]) * 2 + size.0;
        let bottom = little(&data[3..6]) * 2 + size.1;
        if right > width || bottom > height || data[15] & 0xfc != 0 {
            return Err("frame exceeds canvas or has reserved flags");
        }
        frame_chunks(&data[16..], size)?;
        self.frames += 1;
        self.duration = self
            .duration
            .checked_add(little(&data[12..15]))
            .ok_or("animation duration overflow")?;
        if self.frames > MAX_FRAMES || self.duration > MAX_DURATION_MS {
            return Err("animation exceeds limit");
        }
        self.image = true;
        Ok(())
    }
}

/// Checks container bounds, metadata and advertised canvas/frame dimensions before decoding.
/// A successful check still requires bounded WebP bitstream decoding and nontransparent RGBA.
///
/// # Errors
///
/// Returns an error if container structure, bounds, dimensions, chunk ordering,
/// metadata policy, padding, or animation frame/duration limits are invalid.
pub fn check_header(bytes: &[u8]) -> Result<(), &'static str> {
    if bytes.len() > MAX_BYTES
        || bytes.len() < 12
        || &bytes[..4] != b"RIFF"
        || &bytes[8..12] != b"WEBP"
    {
        return Err("invalid or oversized WebP container");
    }
    if u64::from(little(&bytes[4..8])) + 8 != bytes.len() as u64 {
        return Err("WebP RIFF size mismatch");
    }
    let mut offset = 12;
    let mut state = HeaderState::default();
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or("truncated WebP chunk")?;
        let size = little(&header[4..8]) as usize;
        let start = offset + 8;
        let end = start.checked_add(size).ok_or("WebP chunk overflow")?;
        let data = bytes.get(start..end).ok_or("truncated WebP payload")?;
        let kind = &header[..4];
        if matches!(kind, b"VP8 " | b"VP8L") && (state.image || state.animated_header) {
            return Err("unexpected static WebP image");
        }
        match kind {
            b"EXIF" | b"XMP " => return Err("WebP metadata is forbidden"),
            b"VP8X" => state.check_vp8x(data, offset)?,
            b"VP8 " => state.check_vp8(data)?,
            b"VP8L" => state.check_vp8l(data)?,
            b"ANIM" => state.check_anim(data)?,
            b"ANMF" => state.check_anmf(data)?,
            _ => {}
        }
        offset = end.checked_add(size % 2).ok_or("WebP padding overflow")?;
        if offset > bytes.len() || (!size.is_multiple_of(2) && bytes[end] != 0) {
            return Err("invalid WebP padding");
        }
    }
    if !state.image {
        return Err("missing WebP image");
    }
    Ok(())
}
