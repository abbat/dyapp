#![no_main]
//! A preview or profile thumbnail from another user; the corpus seeds are small JPEGs.

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = dyapp_ffi::check_preview(data.to_vec());
});
