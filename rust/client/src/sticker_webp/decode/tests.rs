use super::*;
use image_webp::{ColorType, WebPEncoder};

fn encoded(pixels: &[u8], width: u32, height: u32, color: ColorType) -> Vec<u8> {
    let mut bytes = Vec::new();
    WebPEncoder::new(&mut bytes)
        .encode(pixels, width, height, color)
        .unwrap();
    bytes
}

fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if !data.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}

fn animation(first: &[u8], second: &[u8]) -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(*b"VP8X", &[0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    body.extend(chunk(*b"ANIM", &[0; 6]));
    for image in [first, second] {
        let mut frame = vec![0; 16];
        frame[12] = 100; // 100 ms, 1x1 full-canvas frame.
        frame[15] = 2; // Replace instead of blending.
        frame.extend_from_slice(&image[12..]);
        body.extend(chunk(*b"ANMF", &frame));
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend(body);
    bytes
}

#[test]
fn independent_libwebp_lossy_vector_matches_reference_rgba() {
    // libwebp 1.5.0 WebPEncodeRGB([10,20,30], 1x1, stride 3, quality 80).
    // Expected pixels come from its WebPDecodeRGBA, independently of image-webp.
    let bytes = [
        82, 73, 70, 70, 46, 0, 0, 0, 87, 69, 66, 80, 86, 80, 56, 32, 34, 0, 0, 0, 112, 1, 0, 157,
        1, 42, 1, 0, 1, 0, 1, 64, 38, 37, 148, 2, 116, 1, 64, 0, 0, 254, 252, 55, 129, 87, 247,
        212, 232, 62, 43, 224, 0, 0,
    ];
    let frame = first_frame(&bytes).unwrap();
    assert_eq!((frame.width, frame.height), (1, 1));
    assert_eq!(frame.pixels, [11, 21, 29, 255]);
}

#[test]
fn rgb_and_rgba_are_always_bounded_rgba() {
    for (pixels, color, expected) in [
        (vec![10, 20, 30], ColorType::Rgb8, vec![10, 20, 30, 255]),
        (
            vec![10, 20, 30, 127],
            ColorType::Rgba8,
            vec![10, 20, 30, 127],
        ),
    ] {
        let frame = first_frame(&encoded(&pixels, 1, 1, color)).unwrap();
        assert_eq!((frame.width, frame.height), (1, 1));
        assert_eq!(frame.pixels, expected);
    }
}

#[test]
fn decoded_canvas_obeys_the_dimension_and_rgba_bounds() {
    let bytes = encoded(&vec![100; 512 * 512 * 3], 512, 512, ColorType::Rgb8);
    let frame = first_frame(&bytes).unwrap();
    assert_eq!((frame.width, frame.height), (512, 512));
    assert_eq!(frame.pixels.len(), 512 * 512 * 4);
    let too_wide = encoded(&vec![100; 513 * 3], 513, 1, ColorType::Rgb8);
    assert_eq!(
        first_frame(&too_wide).err(),
        Some("sticker dimensions exceed limit")
    );
}

#[test]
fn fully_transparent_but_not_partial_alpha_is_refused() {
    let bytes = encoded(&[0, 0, 0, 0], 1, 1, ColorType::Rgba8);
    assert_eq!(
        first_frame(&bytes).err(),
        Some("transparent WebP first frame")
    );
    let bytes = encoded(&[0, 0, 0, 0, 0, 0, 0, 1], 2, 1, ColorType::Rgba8);
    assert!(first_frame(&bytes).is_ok());
}

#[test]
fn animation_decodes_only_the_first_canvas() {
    let red = encoded(&[255, 0, 0, 255], 1, 1, ColorType::Rgba8);
    let blue = encoded(&[0, 0, 255, 255], 1, 1, ColorType::Rgba8);
    let frame = first_frame(&animation(&red, &blue)).unwrap();
    assert_eq!(frame.pixels, [255, 0, 0, 255]);
    let transparent = encoded(&[0; 4], 1, 1, ColorType::Rgba8);
    assert!(first_frame(&animation(&transparent, &blue)).is_err());
}

#[test]
fn valid_header_does_not_authorize_corrupt_pixels() {
    let bytes = encoded(&[20, 30, 40], 1, 1, ColorType::Rgb8);
    let payload = &bytes[20..25]; // VP8L signature and dimension header, no pixels.
    let mut corrupt = b"RIFF".to_vec();
    let image = chunk(*b"VP8L", payload);
    corrupt.extend_from_slice(&((image.len() + 4) as u32).to_le_bytes());
    corrupt.extend_from_slice(b"WEBP");
    corrupt.extend(image);
    check_header(&corrupt).unwrap();
    assert!(first_frame(&corrupt).is_err());
    assert!(first_frame(b"not WebP").is_err());
}
