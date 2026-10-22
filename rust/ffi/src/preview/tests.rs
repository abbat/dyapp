use super::*;

/// Deterministic xorshift64, so failures replay.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Smooth gradients with soft bands, roughly how a photo compresses.
fn photo(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = ((f64::from(x + y) / 300.0).sin() * 40.0) as i32;
            rgba.extend([
                (x * 200 / width) as u8,
                (y * 200 / height) as u8,
                (128 + band) as u8,
                255,
            ]);
        }
    }
    rgba
}

fn noise(width: u32, height: u32) -> Vec<u8> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    (0..width * height * 4).map(|_| rng.next() as u8).collect()
}

fn preview(width: u32, height: u32) -> Vec<u8> {
    make_preview(photo(width, height), width, height, MAX_BYTES).unwrap()
}

/// Every segment as (start, marker, length) up to the first scan.
fn segments(jpeg: &[u8]) -> Vec<(usize, u8, usize)> {
    let mut found = Vec::new();
    let mut at = 2;
    while jpeg[at + 1] != 0xDA {
        let len = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        found.push((at, jpeg[at + 1], len));
        at += 2 + len;
    }
    found
}

fn sof(jpeg: &[u8]) -> (usize, usize) {
    let (at, _, len) = *segments(jpeg).iter().find(|s| s.1 == 0xC0).unwrap();
    (at, len)
}

#[test]
fn photo_becomes_320_px_within_24_kib() {
    let jpeg = preview(4000, 3000);
    assert!(jpeg.len() <= MAX_BYTES as usize, "{} bytes", jpeg.len());
    let decoded = check_preview(jpeg).unwrap();
    assert_eq!((decoded.width, decoded.height), (320, 240));
}

#[test]
fn noise_falls_back_to_160_px() {
    // Noise does not fit 24 KiB at 320 px; u32::MAX also shows the 24 KiB cap.
    let jpeg = make_preview(noise(320, 320), 320, 320, u32::MAX).unwrap();
    assert!(jpeg.len() <= MAX_BYTES as usize, "{} bytes", jpeg.len());
    let decoded = check_preview(jpeg).unwrap();
    assert_eq!((decoded.width, decoded.height), (160, 160));
}

#[test]
fn max_bytes_is_honoured_or_refused() {
    if let Ok(jpeg) = make_preview(photo(4000, 3000), 4000, 3000, 4096) {
        assert!(jpeg.len() <= 4096, "{} bytes", jpeg.len());
    }
    assert!(make_preview(noise(320, 320), 320, 320, 100).is_err());
}

#[test]
fn small_and_transparent_sources_keep_their_size() {
    let mut rgba = photo(40, 20);
    rgba.iter_mut()
        .skip(3)
        .step_by(4)
        .for_each(|alpha| *alpha = 0);
    let decoded = check_preview(make_preview(rgba, 40, 20, MAX_BYTES).unwrap()).unwrap();
    assert_eq!((decoded.width, decoded.height), (40, 20));
    // Fully transparent pixels flatten to white.
    assert!(decoded.pixels.iter().all(|&v| v > 245));
}

#[test]
fn pixels_must_match_dimensions() {
    assert!(make_preview(vec![0; 15], 2, 2, MAX_BYTES).is_err());
    assert!(make_preview(Vec::new(), 0, 0, MAX_BYTES).is_err());
}

#[test]
fn output_has_no_metadata() {
    let jpeg = preview(400, 300);
    assert!(
        segments(&jpeg).iter().all(|s| s.1 != 0xE1),
        "APP1 (EXIF/XMP) segment"
    );
    assert!(!jpeg.windows(4).any(|w| w == b"Exif"));
}

#[test]
fn rejects_oversized_wide_corrupt_and_png() {
    let mut big = preview(400, 300);
    big.truncate(big.len() - 2);
    big.resize(25 * 1024 - 2, 0);
    big.extend([0xFF, 0xD9]);
    assert!(check_preview(big).is_err());

    let mut wide = Vec::new();
    JpegEncoder::new_with_quality(&mut wide, 40)
        .encode_image(&RgbImage::new(321, 1))
        .unwrap();
    assert!(check_preview(wide).is_err());

    let mut corrupt = preview(400, 300);
    let (at, _) = sof(&corrupt);
    // Zero components in the frame header.
    corrupt[at + 9] = 0;
    assert!(check_preview(corrupt).is_err());

    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10\x08\x06\0\0\0".to_vec();
    assert!(check_preview(png).is_err());
}

#[test]
fn hostile_corpus_errs_without_panic() {
    let valid = preview(400, 300);
    let mut corpus = Vec::new();

    corpus.push(valid[..valid.len() / 2].to_vec());

    let mut huge = valid.clone();
    let (at, _) = sof(&huge);
    huge[at + 5..at + 9].copy_from_slice(&[0xFF; 4]);
    corpus.push(huge);

    let mut twice = valid.clone();
    let (at, len) = sof(&twice);
    let frame = twice[at..at + 2 + len].to_vec();
    twice.splice(at..at, frame);
    corpus.push(twice);

    // Progressive frame header, then 2000 scans with no tables.
    let mut scans = vec![0xFF, 0xD8, 0xFF, 0xC2, 0, 11, 8, 0, 8, 0, 8, 1, 1, 0x11, 0];
    for _ in 0..2000 {
        scans.extend([0xFF, 0xDA, 0, 8, 1, 1, 0, 0, 0, 0, 0]);
    }
    scans.extend([0xFF, 0xD9]);
    assert!(scans.len() <= MAX_BYTES as usize);
    corpus.push(scans);

    let mut zeros = vec![0xFF, 0xD8];
    zeros.resize(1000, 0);
    zeros.extend([0xFF, 0xD9]);
    corpus.push(zeros);

    for (i, bytes) in corpus.into_iter().enumerate() {
        assert!(check_preview(bytes).is_err(), "corpus entry {i}");
    }
}

#[test]
fn mutated_previews_never_panic() {
    // A small preview keeps 10 000 unoptimized decodes fast.
    let valid = preview(48, 32);
    let mut rng = Rng(0xD1A9_5EED);
    for round in 0..10_000 {
        let mut bytes = valid.clone();
        for _ in 0..1 + rng.next() % 4 {
            let at = (rng.next() % bytes.len() as u64) as usize;
            bytes[at] ^= (rng.next() % 255 + 1) as u8;
        }
        if let Ok(decoded) = check_preview(bytes) {
            assert!(
                decoded.width <= MAX_SIDE && decoded.height <= MAX_SIDE,
                "round {round}"
            );
            assert_eq!(
                decoded.pixels.len(),
                (decoded.width * decoded.height * 4) as usize,
                "round {round}"
            );
        }
    }
}

#[test]
fn network_is_metered_until_reported() {
    assert!(network_metered());
    set_network_metered(false);
    assert!(!network_metered());
    set_network_metered(true);
}
