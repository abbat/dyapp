//! Photo metadata stripping before upload: EXIF, XMP, IPTC, GPS, maker notes and text reveal where
//! and with which device a photo was taken.

use crate::CoreError;
use image::codecs::jpeg::JpegEncoder;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader};
use std::io::Cursor;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
// Image data and colour; everything else (tEXt, zTXt, iTXt, eXIf, tIME, pHYs, ...) is dropped.
const PNG_KEEP: [&[u8; 4]; 11] = [
    b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"cHRM", b"gAMA", b"iCCP", b"sBIT", b"sRGB",
    b"cICP",
];
const ROTATED_QUALITY: u8 = 90;

fn failed(reason: &str) -> CoreError {
    CoreError::Failed(reason.to_string())
}

/// `bytes` of a JPEG or PNG without metadata: only image data, JFIF, Adobe colour transform and
/// ICC colour profile segments are kept, and anything after the end marker is cut (motion-photo
/// videos). A JPEG with an EXIF orientation is decoded, turned upright and re-encoded at quality
/// 90. Errs on other formats, a malformed file, and a PNG with an eXIf orientation; the app then
/// converts the photo to JPEG itself.
#[uniffi::export]
pub fn strip_metadata(bytes: Vec<u8>) -> Result<Vec<u8>, CoreError> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        upright_jpeg(&bytes)
    } else if bytes.starts_with(&PNG_SIGNATURE) {
        png(&bytes)
    } else {
        Err(failed("not a JPEG or PNG"))
    }
}

fn upright_jpeg(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    let error = |e: image::ImageError| CoreError::Failed(e.to_string());
    let mut decoder = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg)
        .into_decoder()
        .map_err(error)?;
    let orientation = decoder.orientation().map_err(error)?;
    if orientation == Orientation::NoTransforms {
        return jpeg(bytes);
    }
    let icc = decoder.icc_profile().map_err(error)?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(error)?;
    image.apply_orientation(orientation);
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, ROTATED_QUALITY);
    if let Some(icc) = icc {
        encoder
            .set_icc_profile(icc)
            .map_err(|e| failed(&e.to_string()))?;
    }
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_with_encoder(encoder)
        .map_err(error)?;
    Ok(out)
}

/// Copies the JPEG segment by segment, entropy-coded data verbatim, dropping APP1 (EXIF, XMP),
/// APP13 (IPTC), COM and every other application segment but JFIF, ICC and Adobe.
fn jpeg(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    let mut out = bytes[..2].to_vec();
    let mut at = 2;
    loop {
        if bytes.get(at) != Some(&0xFF) {
            return Err(failed("bad JPEG marker"));
        }
        // Fill bytes before a marker.
        while bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        let marker = *bytes.get(at + 1).ok_or_else(|| failed("truncated JPEG"))?;
        match marker {
            0xD9 => {
                out.extend_from_slice(&[0xFF, 0xD9]);
                return Ok(out);
            }
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(&[0xFF, marker]);
                at += 2;
                continue;
            }
            _ => {}
        }
        let len = bytes
            .get(at + 2..at + 4)
            .map(|len| usize::from(u16::from_be_bytes([len[0], len[1]])))
            .filter(|&len| len >= 2)
            .ok_or_else(|| failed("bad JPEG segment length"))?;
        let segment = bytes
            .get(at..at + 2 + len)
            .ok_or_else(|| failed("truncated JPEG segment"))?;
        let body = &segment[4..];
        let keep = match marker {
            0xE0 => body.starts_with(b"JFIF\0"),
            0xE2 => body.starts_with(b"ICC_PROFILE\0"),
            0xEE => body.starts_with(b"Adobe"),
            0xE1..=0xEF | 0xFE => false,
            _ => true,
        };
        if keep {
            out.extend_from_slice(segment);
        }
        at += segment.len();
        if marker == 0xDA {
            // Entropy-coded data runs to the next marker other than a stuffed 0xFF00 or RSTn.
            let start = at;
            while at < bytes.len()
                && !(bytes[at] == 0xFF
                    && bytes
                        .get(at + 1)
                        .is_some_and(|&next| next != 0x00 && !(0xD0..=0xD7).contains(&next)))
            {
                at += 1;
            }
            out.extend_from_slice(&bytes[start..at]);
        }
    }
}

/// Copies the image and colour chunks verbatim, CRCs included, up to IEND.
fn png(bytes: &[u8]) -> Result<Vec<u8>, CoreError> {
    let mut out = PNG_SIGNATURE.to_vec();
    let mut at = PNG_SIGNATURE.len();
    loop {
        let header = bytes
            .get(at..at + 8)
            .ok_or_else(|| failed("truncated PNG"))?;
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        let chunk = at
            .checked_add(12 + len)
            .and_then(|end| bytes.get(at..end))
            .ok_or_else(|| failed("truncated PNG chunk"))?;
        if kind == b"eXIf"
            && Orientation::from_exif_chunk(&chunk[8..8 + len])
                .is_some_and(|o| o != Orientation::NoTransforms)
        {
            return Err(failed("rotated PNG"));
        }
        if PNG_KEEP.iter().any(|keep| keep.as_slice() == kind) {
            out.extend_from_slice(chunk);
        }
        if kind == b"IEND" {
            return Ok(out);
        }
        at += chunk.len();
    }
}

#[cfg(test)]
mod tests;
