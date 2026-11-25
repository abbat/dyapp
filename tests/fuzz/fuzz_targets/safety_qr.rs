#![no_main]
//! Safety QR payloads and bounded grayscale frames scanned from a contact.

use dyapp_identity::safety::qr_matches_peer_ids;

const MINE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const THEIRS: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const MAX_SIDE: usize = 768;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = data.split_first() else {
        return;
    };
    if selector & 1 == 0 {
        let _ = qr_matches_peer_ids(data, MINE, THEIRS);
        return;
    }
    if data.len() < 4 {
        return;
    }
    let width = usize::from(u16::from_le_bytes(data[0..2].try_into().unwrap()));
    let height = usize::from(u16::from_le_bytes(data[2..4].try_into().unwrap()));
    let pixels = &data[4..];
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || pixels.len() != width * height
    {
        return;
    }
    let mut decoder = quircs::Quirc::default();
    for code in decoder.identify(width, height, pixels) {
        let Ok(code) = code else {
            continue;
        };
        let Ok(decoded) = code.decode() else {
            continue;
        };
        let _ = qr_matches_peer_ids(&decoded.payload, MINE, THEIRS);
    }
});
