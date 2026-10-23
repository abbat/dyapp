use super::*;
use image::{Rgb, RgbImage};

/// 16 x 8: the left half red, the right half blue.
fn plain_jpeg() -> Vec<u8> {
    let image = RgbImage::from_fn(16, 8, |x, _| {
        if x < 8 {
            Rgb([255, 0, 0])
        } else {
            Rgb([0, 0, 255])
        }
    });
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&image)
        .unwrap();
    jpeg
}

/// Big-endian EXIF: IFD0 with `orientation` and a GPS IFD holding latitude ref "N".
fn exif(orientation: u16) -> Vec<u8> {
    let mut tiff = b"MM\0\x2A\0\0\0\x08".to_vec();
    tiff.extend([0, 2]);
    tiff.extend([0x01, 0x12, 0, 3, 0, 0, 0, 1]);
    tiff.extend(orientation.to_be_bytes());
    tiff.extend([0, 0]);
    tiff.extend([0x88, 0x25, 0, 4, 0, 0, 0, 1, 0, 0, 0, 38]);
    tiff.extend([0, 0, 0, 0]);
    tiff.extend([0, 1, 0, 1, 0, 2, 0, 0, 0, 2, b'N', 0, 0, 0, 0, 0, 0, 0]);
    tiff
}

fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
    let mut segment = vec![0xFF, marker];
    segment.extend(u16::try_from(body.len() + 2).unwrap().to_be_bytes());
    segment.extend_from_slice(body);
    segment
}

/// `jpeg` with EXIF, XMP, IPTC and a comment after SOI and a trailer after EOI.
fn with_metadata(jpeg: &[u8], orientation: u16) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    out.extend(segment(
        0xE1,
        &[b"Exif\0\0".as_slice(), &exif(orientation)].concat(),
    ));
    out.extend(segment(0xE1, b"http://ns.adobe.com/xap/1.0/\0<x:xmpmeta/>"));
    out.extend(segment(0xED, b"Photoshop 3.0\0IPTC"));
    out.extend(segment(0xFE, b"Canon EOS"));
    out.extend_from_slice(&jpeg[2..]);
    out.extend_from_slice(b"motion photo video");
    out
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn jpeg_loses_its_metadata_and_keeps_its_pixels() {
    let plain = plain_jpeg();
    let stripped = strip_metadata(with_metadata(&plain, 1)).unwrap();
    assert_eq!(stripped, plain);
}

#[test]
fn rotated_jpeg_is_turned_upright() {
    let stripped = strip_metadata(with_metadata(&plain_jpeg(), 6)).unwrap();
    for needle in [b"Exif".as_slice(), b"xmpmeta", b"IPTC", b"Canon", b"motion"] {
        assert!(!contains(&stripped, needle));
    }
    let image = image::load_from_memory_with_format(&stripped, ImageFormat::Jpeg)
        .unwrap()
        .to_rgb8();
    // Orientation 6 turns the image 90 degrees clockwise: the left half becomes the top.
    assert_eq!(image.dimensions(), (8, 16));
    let [r, _, b] = image.get_pixel(4, 2).0;
    assert!(r > 200 && b < 60, "top is red");
    let [r, _, b] = image.get_pixel(4, 13).0;
    assert!(r < 60 && b > 200, "bottom is blue");
    assert_eq!(strip_metadata(stripped.clone()).unwrap(), stripped);
}

fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |crc, &byte| {
        (0..8).fold(crc ^ u32::from(byte), |crc, _| {
            (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg())
        })
    })
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);
    chunk.extend(crc32(&chunk[4..]).to_be_bytes());
    chunk
}

/// A 1 x 1 green PNG: signature, IHDR, IDAT and IEND.
fn plain_png() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    let ihdr = [0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0];
    // zlib, one stored block: filter byte 0 and the pixel 00 FF 00; Adler-32 0x02020100.
    let idat = [
        0x78, 0x01, 0x01, 4, 0, 0xFB, 0xFF, 0, 0, 0xFF, 0, 0x02, 0x02, 0x01, 0x00,
    ];
    vec![
        (b"IHDR", ihdr.to_vec()),
        (b"IDAT", idat.to_vec()),
        (b"IEND", Vec::new()),
    ]
}

fn png_of(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    for (kind, data) in chunks {
        png.extend(chunk(kind, data));
    }
    png
}

#[test]
fn png_loses_its_text_exif_and_time() {
    let mut chunks = plain_png();
    chunks.insert(1, (b"tEXt", b"Author\0someone".to_vec()));
    chunks.insert(1, (b"iTXt", b"GPS\0\0\0\0\0N 52.5".to_vec()));
    chunks.insert(1, (b"eXIf", exif(1)));
    chunks.insert(3, (b"tIME", vec![7, 234, 10, 23, 12, 0, 0]));
    let mut png = png_of(&chunks);
    png.extend_from_slice(b"trailer");
    assert_eq!(strip_metadata(png).unwrap(), png_of(&plain_png()));
}

#[test]
fn refuses_what_it_cannot_strip() {
    let mut rotated = plain_png();
    rotated.insert(1, (b"eXIf", exif(6)));
    assert!(strip_metadata(png_of(&rotated)).is_err());
    assert!(strip_metadata(b"GIF89a".to_vec()).is_err());
    let plain = plain_jpeg();
    assert!(strip_metadata(plain[..plain.len() - 2].to_vec()).is_err());
    assert!(strip_metadata(plain[..40].to_vec()).is_err());
    let png = png_of(&plain_png());
    assert!(strip_metadata(png[..png.len() - 12].to_vec()).is_err());
}
