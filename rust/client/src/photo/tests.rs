use super::*;

#[test]
fn empty_mismatched_and_overflowing_dimensions_are_rejected() {
    for (pixels, width, height) in [
        (&[][..], 0, 0),
        (&[0; 15][..], 2, 2),
        (&[][..], u32::MAX, u32::MAX),
    ] {
        let error = make_preview(pixels, width, height, MAX_BYTES).expect_err("invalid dimensions");
        assert_eq!(error.to_string(), "pixels do not match width and height");
    }
}

#[test]
fn small_transparent_image_is_metadata_free_and_flattens_to_white() {
    let jpeg = make_preview(&[0; 4 * 4 * 4], 4, 4, MAX_BYTES).expect("transparent image");
    assert!(jpeg.len() <= 24 * 1024);
    assert!(!jpeg.windows(4).any(|bytes| bytes == b"Exif"));
    assert_eq!(frame_size(&jpeg).expect("frame"), (4, 4));
    let decoded = check_preview(&jpeg).expect("bounded decode");
    assert_eq!((decoded.width, decoded.height), (4, 4));
    assert_eq!(decoded.pixels.len(), 4 * 4 * 4);
    assert!(decoded.pixels.iter().all(|&value| value > 245));
}

#[test]
fn oversized_inputs_are_rejected_and_hostile_previews_do_not_panic() {
    assert!(check_preview(&vec![0; 24 * 1024 + 1]).is_err());
    let hostile = include_bytes!(
        "../../../../tests/fuzz/corpus/preview/b31fdd103ccba5f493ba6602325173fdcbca3eb1"
    );
    assert!(std::panic::catch_unwind(|| check_preview(hostile)).is_ok());
    assert!(check_preview(b"not JPEG").is_err());
}
