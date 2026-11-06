#![no_main]
//! Sticker import headers, sizes, paths and terminators without filesystem extraction.

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Ok(files) = dyapp_client::sticker_tar::read(data) {
        assert!(files.len() <= 121);
        for (name, bytes) in files {
            assert!(!name.contains(['/', '\\']));
            assert!(bytes.len() <= 256 * 1024);
        }
    }
});
