use super::{profile_acceptable, ProfileImage};

/// A framing-only witness: dimension admission must not allocate or decode the claimed pixels.
fn frame(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xc0, 0, 11, 8];
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[
        1, 1, 0x11, 0, 0xff, 0xda, 0, 8, 1, 1, 0, 0, 63, 0, 0xff, 0xd9,
    ]);
    bytes
}

fn padded(length: usize) -> Vec<u8> {
    let mut bytes = frame(3, 2);
    assert!(length >= bytes.len());
    bytes.resize(length, 0);
    bytes
}

#[test]
fn profile_pixel_and_editor_bounds_are_distinct_and_inclusive() {
    use ProfileImage::{Original, PreparedOriginal, Thumbnail};

    assert!(profile_acceptable(&frame(8_000, 8_000), Original));
    assert!(!profile_acceptable(&frame(8_001, 8_000), Original));
    assert!(!profile_acceptable(&frame(u16::MAX, u16::MAX), Original));
    for kind in [Original, PreparedOriginal, Thumbnail] {
        assert!(!profile_acceptable(&frame(0, 2), kind));
        assert!(!profile_acceptable(&frame(2, 0), kind));
        assert!(!profile_acceptable(b"not an image", kind));
        assert!(!profile_acceptable(
            include_bytes!("../attachment/original/validate/three-by-two.png"),
            kind,
        ));
    }
    assert!(profile_acceptable(&frame(320, 320), Thumbnail));
    assert!(!profile_acceptable(&frame(321, 2), Thumbnail));
    assert!(profile_acceptable(&frame(321, 2), Original));
    assert!(profile_acceptable(&frame(2_048, 2), PreparedOriginal));
    assert!(!profile_acceptable(&frame(2_049, 2), PreparedOriginal));
    assert!(profile_acceptable(&frame(2_049, 2), Original));
}

#[test]
fn profile_byte_limits_apply_before_decoding_and_differ_by_media_kind() {
    use ProfileImage::{Original, PreparedOriginal, Thumbnail};

    let thumbnail = padded(24 * 1024);
    let over_thumbnail = padded(24 * 1024 + 1);
    assert!(profile_acceptable(&thumbnail, Thumbnail));
    assert!(!profile_acceptable(&over_thumbnail, Thumbnail));
    assert!(profile_acceptable(&over_thumbnail, Original));
    for kind in [Original, PreparedOriginal] {
        assert!(profile_acceptable(&padded(1024 * 1024), kind));
        assert!(!profile_acceptable(&padded(1024 * 1024 + 1), kind));
    }
}
