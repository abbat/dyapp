#![no_main]
//! Decoded RGBA input, dimensions and purpose for the shared photo preparation path.

use dyapp_ffi::PhotoPurpose;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((selector, data)) = data.split_first() else {
        return;
    };
    if data.len() < 8 {
        return;
    }
    let width = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let purpose = if selector & 1 == 0 {
        PhotoPurpose::Profile
    } else {
        PhotoPurpose::Chat
    };
    let _ = dyapp_ffi::prepare_photo_pixels(data[8..].to_vec(), width, height, purpose);
});
