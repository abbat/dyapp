#![no_main]
//! A profile link pasted into a chat, including malformed UTF-8 at the boundary.

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = dyapp_identity::profile_link::parse_profile_link(text);
    }
});
