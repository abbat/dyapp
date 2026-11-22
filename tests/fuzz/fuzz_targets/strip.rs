#![no_main]
//! JPEG/PNG metadata stripping, including bounded decoding of oriented JPEGs.

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = dyapp_ffi::strip_metadata(data.to_vec());
});
