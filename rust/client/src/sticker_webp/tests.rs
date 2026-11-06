use super::*;

fn chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = kind.to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if !payload.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}

fn webp(chunks: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(chunks);
    out
}

#[test]
fn small_lossless_header_passes_but_is_not_a_decode_proof() {
    let file = webp(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]));
    assert!(check_header(&file).is_ok());
    for end in 0..file.len() {
        assert!(check_header(&file[..end]).is_err());
    }
}

fn animated(frame: &[u8]) -> Vec<u8> {
    let mut chunks = chunk(b"VP8X", &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    chunks.extend_from_slice(&chunk(b"ANIM", &[0; 6]));
    chunks.extend_from_slice(&chunk(b"ANMF", frame));
    webp(&chunks)
}

#[test]
fn frame_rectangle_and_reserved_flags_are_checked() {
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]));
    assert!(check_header(&animated(&frame)).is_ok());
    frame[0] = 1;
    assert_eq!(
        check_header(&animated(&frame)),
        Err("frame exceeds canvas or has reserved flags")
    );
    frame[0] = 0;
    frame[15] = 4;
    assert!(check_header(&animated(&frame)).is_err());
    frame[15] = 0;
    frame[12..15].copy_from_slice(&[0xff, 0xff, 0]);
    assert_eq!(
        check_header(&animated(&frame)),
        Err("animation exceeds limit")
    );
}

#[test]
fn valid_animation_envelope_does_not_hide_bad_nested_image() {
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0xff, 0xff, 0xff, 0x0f]));
    assert_eq!(
        check_header(&animated(&frame)),
        Err("sticker dimensions exceed limit")
    );
}

#[test]
fn frame_bitstream_must_match_rectangle_and_alpha_layout() {
    let mut frame = vec![0; 16];
    // A two-pixel bitstream inside the advertised one-pixel frame.
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 1, 0, 0, 0]));
    assert_eq!(
        check_header(&animated(&frame)),
        Err("frame VP8L header mismatch")
    );
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0x20]));
    assert!(check_header(&animated(&frame)).is_err());
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"ALPH", &[0]));
    frame.extend_from_slice(&chunk(b"ALPH", &[0]));
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]));
    assert!(check_header(&animated(&frame)).is_err());
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"ALPH", &[0]));
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]));
    assert!(check_header(&animated(&frame)).is_err());
}

#[test]
fn animation_frame_count_and_byte_limits_are_enforced() {
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]));
    let mut chunks = chunk(b"VP8X", &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    chunks.extend_from_slice(&chunk(b"ANIM", &[0; 6]));
    for _ in 0..MAX_FRAMES {
        chunks.extend_from_slice(&chunk(b"ANMF", &frame));
    }
    assert!(check_header(&webp(&chunks)).is_ok());
    chunks.extend_from_slice(&chunk(b"ANMF", &frame));
    assert_eq!(check_header(&webp(&chunks)), Err("animation exceeds limit"));
    let mut chunks = chunk(b"VP8L", &[0x2f, 0, 0, 0, 0]);
    chunks.extend_from_slice(&chunk(b"JUNK", &vec![0; MAX_BYTES]));
    assert_eq!(
        check_header(&webp(&chunks)),
        Err("invalid or oversized WebP container")
    );
}

#[test]
fn metadata_large_dimensions_and_nested_frames_are_refused() {
    assert!(check_header(&webp(&chunk(b"EXIF", b"x"))).is_err());
    assert!(check_header(&webp(&chunk(b"VP8L", &[0x2f, 0xff, 0xff, 0xff, 0x0f]))).is_err());
    let mut frame = vec![0; 16];
    frame.extend_from_slice(&chunk(b"VP8L", &[0x2f, 0xff, 0xff, 0xff, 0x0f]));
    assert!(check_header(&webp(&chunk(b"ANMF", &frame))).is_err());
    let mut nested = vec![0; 16];
    nested.extend_from_slice(&chunk(b"ANMF", &[0; 16]));
    assert!(check_header(&webp(&chunk(b"ANMF", &nested))).is_err());
}
