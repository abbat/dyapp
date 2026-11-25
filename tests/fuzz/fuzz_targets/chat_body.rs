#![no_main]
//! Authenticated MLS application plaintext: padding and every chat-body decoder branch.

use dyapp_client::chat::{self, Body};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some(plaintext) = chat::unpad(data) else {
        return;
    };
    if let Ok(body) = Body::decode(plaintext) {
        assert!(Body::decode(&body.encode()).is_ok());
    }
});
