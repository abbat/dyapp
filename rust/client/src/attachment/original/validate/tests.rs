use super::*;
use image::{ImageFormat, RgbImage};
use std::io::Cursor;

fn image(format: ImageFormat) -> Vec<u8> {
    if format == ImageFormat::Png {
        return include_bytes!("three-by-two.png").to_vec();
    }
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::new(3, 2).write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

#[test]
fn jpeg_and_png_accept_unspecified_metadata_but_check_each_specified_dimension() {
    for format in [ImageFormat::Jpeg, ImageFormat::Png] {
        let bytes = image(format);
        for (width, height) in [(3, 2), (0, 0), (3, 0), (0, 2)] {
            assert!(validate(&bytes, width, height).is_ok());
        }
        for (width, height) in [(4, 2), (3, 4), (4, 0), (0, 4)] {
            assert!(matches!(
                validate(&bytes, width, height),
                Err(LookupError::Corrupted)
            ));
        }
        for invalid in [b"not an image".as_slice(), &bytes[..12]] {
            assert!(validate(invalid, 0, 0).is_err());
        }
    }
}

fn jpeg_dimensions(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = image(ImageFormat::Jpeg);
    let sof = bytes
        .windows(2)
        .position(|pair| pair == [0xff, 0xc0])
        .unwrap();
    bytes[sof + 5..sof + 7].copy_from_slice(&height.to_be_bytes());
    bytes[sof + 7..sof + 9].copy_from_slice(&width.to_be_bytes());
    bytes
}

fn png_dimensions(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = image(ImageFormat::Png);
    bytes[16..20].copy_from_slice(&u32::from(width).to_be_bytes());
    bytes[20..24].copy_from_slice(&u32::from(height).to_be_bytes());
    let crc = png::crc32(&bytes[12..29]).to_be_bytes();
    bytes[29..33].copy_from_slice(&crc);
    bytes
}

#[test]
fn actual_pixels_are_bounded_even_when_the_descriptor_omits_dimensions() {
    for dimensions in [jpeg_dimensions, png_dimensions] {
        assert!(validate(&dimensions(8000, 8000), 0, 0).is_ok());
        assert!(validate(&dimensions(8001, 8000), 0, 0).is_err());
        assert!(validate(&dimensions(0, 2), 0, 0).is_err());
        assert!(validate(&dimensions(3, 0), 0, 0).is_err());
    }
    let bytes = image(ImageFormat::Jpeg);
    assert!(validate(&bytes[..bytes.len() - 2], 3, 2).is_err());
}

#[test]
fn png_crc_and_header_fields_are_checked_before_native_decoding() {
    assert_eq!(png::crc32(b"123456789"), 0xcbf4_3926);
    let original = image(ImageFormat::Png);
    for (offset, value) in [
        (8, 1),
        (12, b'X'),
        (24, 0),
        (25, 1),
        (26, 1),
        (27, 1),
        (28, 2),
    ] {
        let mut bytes = original.clone();
        bytes[offset] = value;
        let crc = png::crc32(&bytes[12..29]).to_be_bytes();
        bytes[29..33].copy_from_slice(&crc);
        assert!(validate(&bytes, 3, 2).is_err());
    }
    let mut changed_crc = original.clone();
    changed_crc[29] ^= 1;
    assert!(validate(&changed_crc, 3, 2).is_err());
    assert!(validate(&original[..original.len() - 1], 3, 2).is_err());
}

#[test]
fn png_interlace_accepts_adam7_but_rejects_reserved_methods() {
    // A real 1x1 RGBA PNG: Adam7's first pass has one pixel, all later passes are empty.
    let original = include_bytes!("one-by-one-adam7.png");
    assert_eq!(original[28], 1);
    for (method, accepted) in [(0, true), (1, true), (2, false), (255, false)] {
        let mut bytes = original.to_vec();
        bytes[28] = method;
        let crc = png::crc32(&bytes[12..29]).to_be_bytes();
        bytes[29..33].copy_from_slice(&crc);
        for (width, height) in [(1, 1), (0, 0), (1, 0), (0, 1)] {
            assert_eq!(
                validate(&bytes, width, height).is_ok(),
                accepted,
                "interlace={method}, descriptor={width}x{height}"
            );
        }
    }
}

// COM padding keeps the JPEG complete without decoding millions of pixels.
fn padded_jpeg(length: usize) -> Vec<u8> {
    let original = image(ImageFormat::Jpeg);
    let mut bytes = original[..2].to_vec();
    while length - bytes.len() - (original.len() - 2) > 0 {
        let remaining = length - bytes.len() - (original.len() - 2);
        let segment = remaining.min(usize::from(u16::MAX) + 2);
        let segment = if remaining > segment && remaining - segment < 4 {
            segment - 4
        } else {
            segment
        };
        assert!(segment >= 4);
        bytes.extend_from_slice(&[0xff, 0xfe]);
        bytes.extend_from_slice(&u16::try_from(segment - 2).unwrap().to_be_bytes());
        bytes.resize(bytes.len() + segment - 4, b'x');
    }
    bytes.extend_from_slice(&original[2..]);
    assert_eq!(bytes.len(), length);
    bytes
}

#[test]
fn incoming_byte_budget_includes_all_chunk_tags_and_is_not_the_composer_budget() {
    // 96 chunks each require a 16-byte tag within the literal 6 MiB node limit.
    let exact = padded_jpeg(6 * 1024 * 1024 - 96 * 16);
    assert_eq!(sealed_size(exact.len()), Some(6 * 1024 * 1024));
    assert!(exact.len() > 6_000_000);
    assert!(validate(&exact, 3, 2).is_ok());
    let over = padded_jpeg(exact.len() + 1);
    assert!(matches!(validate(&over, 3, 2), Err(LookupError::Corrupted)));
}
