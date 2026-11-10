use super::*;
use image::{ImageFormat, RgbImage};
use std::io::Cursor;

fn jpeg() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::new(3, 2)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    bytes.into_inner()
}

// Legal COM segments exercise byte limits without allocating millions of decoded pixels.
fn padded_jpeg(length: usize) -> Vec<u8> {
    let original = jpeg();
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
fn chat_byte_limit_is_inclusive_and_profile_budget_stays_separate() {
    let bytes = padded_jpeg(6_000_000);
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (3, 2));
    assert_eq!(validate_original(&bytes, 3, 2, Purpose::Chat), Ok(()));
    assert_eq!(
        validate_original(&bytes, 3, 2, Purpose::Profile),
        Err(PrepareError::TooLarge)
    );
    assert_eq!(
        validate_original(&padded_jpeg(6_000_001), 3, 2, Purpose::Chat),
        Err(PrepareError::TooLarge)
    );
    let profile = padded_jpeg(1024 * 1024);
    assert_eq!(validate_original(&profile, 3, 2, Purpose::Profile), Ok(()));
    assert_eq!(
        validate_original(&padded_jpeg(1024 * 1024 + 1), 3, 2, Purpose::Profile),
        Err(PrepareError::TooLarge)
    );
}

#[test]
fn invalid_frames_and_descriptor_mismatches_are_refused() {
    let bytes = jpeg();
    for (width, height) in [(0, 2), (3, 0), (2, 3), (4, 2)] {
        assert_eq!(
            validate_original(&bytes, width, height, Purpose::Chat),
            Err(PrepareError::Unsupported)
        );
    }
    for invalid in [b"not an image".as_slice(), &bytes[..bytes.len() - 2]] {
        assert_eq!(
            validate_original(invalid, 3, 2, Purpose::Chat),
            Err(PrepareError::Unsupported)
        );
    }
}

#[test]
fn oversized_frames_are_refused_before_native_decode() {
    let mut bytes = jpeg();
    let sof = bytes
        .windows(2)
        .position(|marker| marker == [0xff, 0xc0])
        .unwrap();
    bytes[sof + 5..sof + 9].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    assert_eq!(
        validate_original(&bytes, 65535, 65535, Purpose::Chat),
        Err(PrepareError::TooLarge)
    );
}
