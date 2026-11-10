use super::*;

#[test]
fn native_decode_plans_preserve_purpose_and_source_boundaries() {
    assert_eq!(photo_picked_byte_limit(), 64 * 1024 * 1024);
    for (purpose, side) in [(PhotoPurpose::Profile, 2048), (PhotoPurpose::Chat, 2560)] {
        assert_eq!(
            photo_decode_plan("image/png".into(), 8000, 8000, 64 * 1024 * 1024, purpose),
            Ok(PhotoDecodePlan {
                width: side,
                height: side
            })
        );
    }
    assert_eq!(
        photo_decode_plan(
            "image/png".into(),
            1,
            1,
            64 * 1024 * 1024 + 1,
            PhotoPurpose::Profile
        ),
        Err(PhotoPreparationError::TooLarge)
    );
    assert_eq!(
        photo_decode_plan("image/gif".into(), 1, 1, 1, PhotoPurpose::Chat),
        Err(PhotoPreparationError::Unsupported)
    );
}

#[test]
fn prepared_native_records_and_typed_failures_match_core() {
    for purpose in [PhotoPurpose::Profile, PhotoPurpose::Chat] {
        let made = prepare_photo_pixels(vec![0; 16], 2, 2, purpose).unwrap();
        assert_eq!((made.width, made.height), (2, 2));
        assert_eq!(
            crate::check_preview(made.full).unwrap().pixels,
            vec![255; 16]
        );
        assert_eq!(
            crate::check_preview(made.thumbnail).unwrap().pixels,
            vec![255; 16]
        );
    }
    assert_eq!(
        prepare_photo_pixels(Vec::new(), 2049, 1, PhotoPurpose::Profile).unwrap_err(),
        PhotoPreparationError::TooLarge
    );
    assert_eq!(
        prepare_photo_pixels(vec![0; 15], 2, 2, PhotoPurpose::Chat).unwrap_err(),
        PhotoPreparationError::Unsupported
    );
}
