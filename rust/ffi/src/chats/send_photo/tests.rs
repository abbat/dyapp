use super::*;
use image::{ImageFormat, RgbImage};
use std::io::Cursor;

#[test]
fn prepared_jpeg_roundtrips_and_invalid_input_is_refused() {
    let mut bytes = Cursor::new(Vec::new());
    RgbImage::new(3, 2)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    let bytes = bytes.into_inner();
    let (media, blob) = sealed(&bytes, Vec::new(), 3, 2).unwrap();
    assert_eq!(attachment::open(&media, &blob).unwrap(), bytes);
    assert_eq!((media.width, media.height), (3, 2));
    assert_eq!(media.mime, "image/jpeg");
    assert!(sealed(&bytes, Vec::new(), 2, 3).is_err());
    assert!(sealed(&bytes, Vec::new(), 0, 2).is_err());
    assert_eq!(
        sealed(b"GIF89a", Vec::new(), 1, 1).unwrap_err().to_string(),
        "unsupported"
    );
    assert_eq!(
        sealed(&[0xff, 0xd8, 0], Vec::new(), 1, 1)
            .unwrap_err()
            .to_string(),
        "corrupted"
    );
    assert!(sealed(&bytes, vec![1; attachment::MAX_PREVIEW_BYTES + 1], 3, 2).is_err());
}

#[test]
fn chat_original_over_profile_budget_roundtrips() {
    let mut image = RgbImage::new(768, 768);
    let mut state = 1_u64;
    for pixel in image.pixels_mut() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let bytes = state.to_be_bytes();
        *pixel = image::Rgb([bytes[0], bytes[1], bytes[2]]);
    }
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 100)
        .encode_image(&image)
        .unwrap();
    assert!(bytes.len() > 1024 * 1024);
    assert!(bytes.len() < 6_000_000);
    assert!(!crate::peer_photos::acceptable(&bytes, true));
    let (media, blob) = sealed(&bytes, Vec::new(), 768, 768).unwrap();
    assert_eq!(attachment::open(&media, &blob).unwrap(), bytes);
}

#[test]
fn node_refusals_keep_distinct_native_codes() {
    let code = |status| send_error(session::Error::Client(Error::Status(status))).to_string();
    assert_eq!(code(Status::Full), "full");
    assert_eq!(code(Status::TooLarge), "tooLarge");
    assert_eq!(code(Status::NotFound), "peerUnavailable");
    assert_ne!(code(Status::Denied), "full");
}
