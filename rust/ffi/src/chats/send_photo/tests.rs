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
fn node_refusals_keep_distinct_native_codes() {
    let code = |status| send_error(session::Error::Client(Error::Status(status))).to_string();
    assert_eq!(code(Status::Full), "full");
    assert_eq!(code(Status::TooLarge), "tooLarge");
    assert_eq!(code(Status::NotFound), "peerUnavailable");
    assert_ne!(code(Status::Denied), "full");
}
