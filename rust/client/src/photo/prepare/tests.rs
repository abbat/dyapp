use super::*;
use crate::photo::{check_preview, frame_size};

#[test]
fn source_boundaries_and_purpose_targets_are_checked_before_decoding() {
    for (purpose, side) in [(Purpose::Profile, 2048), (Purpose::Chat, 2560)] {
        assert_eq!(
            decode_plan("image/png", 8000, 8000, 64 * 1024 * 1024, purpose),
            Ok(DecodePlan {
                width: side,
                height: side
            })
        );
    }
    for (width, height, bytes) in [(8001, 8000, 1), (1, 1, 64 * 1024 * 1024 + 1)] {
        assert_eq!(
            decode_plan("image/png", width, height, bytes, Purpose::Profile),
            Err(PrepareError::TooLarge)
        );
    }
}

#[test]
fn unsupported_headers_fail_and_small_frames_are_not_upscaled() {
    assert_eq!(
        decode_plan("image/gif", 1, 1, 1, Purpose::Chat),
        Err(PrepareError::Unsupported)
    );
    assert_eq!(
        decode_plan("image/png", 0, 1, 1, Purpose::Chat),
        Err(PrepareError::Unsupported)
    );
    assert_eq!(
        decode_plan("image/jpeg", 3, 2, 1, Purpose::Profile),
        Ok(DecodePlan {
            width: 3,
            height: 2
        })
    );
}

#[test]
fn preparation_keeps_small_dimensions_and_flattens_once_without_metadata() {
    for (purpose, budget) in [(Purpose::Profile, 1_048_576), (Purpose::Chat, 6_000_000)] {
        let made = prepare_photo(&[0; 4 * 4 * 4], 4, 4, purpose).expect("small transparent photo");
        assert_eq!((made.width, made.height), (4, 4));
        assert_eq!(frame_size(&made.full).unwrap(), (4, 4));
        assert!(made.full.len() <= budget);
        assert!(!made.full.windows(4).any(|bytes| bytes == b"Exif"));
        let decoded = check_preview(&made.full).unwrap();
        assert!(decoded.pixels.iter().all(|&value| value > 245));
        assert_eq!(
            check_preview(&made.thumbnail).unwrap().pixels,
            decoded.pixels
        );
    }
    assert_eq!(
        prepare_photo(&[], 2049, 1, Purpose::Profile).unwrap_err(),
        PrepareError::TooLarge
    );
    assert_eq!(
        prepare_photo(&[0; 15], 2, 2, Purpose::Chat).unwrap_err(),
        PrepareError::Unsupported
    );
}

#[test]
fn straight_alpha_is_blended_once_before_jpeg_encoding() {
    let made = prepare_photo(&[255, 0, 0, 128].repeat(64), 8, 8, Purpose::Chat).unwrap();
    let decoded = check_preview(&made.full).unwrap();
    // JPEG is lossy: allow a small quantization error around straight-alpha red on white.
    for pixel in decoded.pixels.as_chunks::<4>().0 {
        assert!(pixel[0] >= 250);
        assert!((123..=131).contains(&pixel[1]));
        assert!((123..=131).contains(&pixel[2]));
        assert_eq!(pixel[3], 255);
    }
}

fn noise() -> RgbImage {
    let mut state = 0x1234_5678u32;
    RgbImage::from_fn(48, 48, |_, _| {
        image::Rgb(std::array::from_fn(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        }))
    })
}

#[test]
fn real_encoder_fallback_uses_the_first_fitting_quality_then_side() {
    let image = noise();
    let high = jpeg(&image, 85).unwrap();
    let lower = jpeg(&image, 55).unwrap();
    assert!(
        high.len() > lower.len(),
        "fixture must require quality fallback"
    );
    let (made, width, height) = fitted(&image, &[48, 24], &[85, 55], lower.len()).unwrap();
    assert_eq!((width, height), (48, 48));
    assert_eq!(made, lower);
    let small = imageops::resize(&image, 24, 24, FilterType::Triangle);
    let small_jpeg = jpeg(&small, 85).unwrap();
    assert!(
        lower.len() > small_jpeg.len(),
        "fixture must require side fallback"
    );
    let (made, width, height) = fitted(&image, &[48, 24], &[85, 55], small_jpeg.len()).unwrap();
    assert_eq!((width, height), (24, 24));
    assert_eq!(made, small_jpeg);
    assert_eq!(
        fitted(&image, &[48, 24], &[85, 55], 1),
        Err(PrepareError::TooLarge)
    );
}
