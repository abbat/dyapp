use super::*;

fn decode(matrix: &QrMatrix) -> Vec<u8> {
    let side = usize::try_from(matrix.size).unwrap();
    assert_eq!(matrix.modules.len(), side * side);
    // QR requires four quiet modules on every edge; scale each module for the independent reader.
    let scale = 4;
    let width = (side + 8) * scale;
    let mut pixels = vec![255; width * width];
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let x = (index % width) / scale;
        let y = (index / width) / scale;
        if (4..side + 4).contains(&x) && (4..side + 4).contains(&y) {
            *pixel = if matrix.modules[(y - 4) * side + x - 4] {
                0
            } else {
                255
            };
        }
    }
    let mut decoder = quircs::Quirc::default();
    let codes = decoder.identify(width, width, &pixels).collect::<Vec<_>>();
    assert_eq!(codes.len(), 1);
    codes
        .into_iter()
        .next()
        .unwrap()
        .unwrap()
        .decode()
        .unwrap()
        .payload
}

#[test]
fn public_text_and_binary_payloads_roundtrip_through_an_independent_decoder() {
    for payload in [
        b"public device link code".as_slice(),
        b"\0\xff\x80binary\0",
        b"",
    ] {
        let qr = matrix(payload).unwrap();
        assert_eq!(decode(&qr), payload);
        assert_eq!(Some(qr), matrix(payload));
    }
}

#[test]
fn oversized_public_payload_is_refused_instead_of_truncated() {
    assert!(matrix(&vec![b'x'; 4096]).is_none());
}
