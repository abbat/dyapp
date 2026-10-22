#![no_main]
//! SFrame video frames from other call members; the first byte is the metadata length.

use dyapp_fuzz::split;
use dyapp_video::sframe::Receiver;
use std::sync::{LazyLock, Mutex};

/// Keys for members 0 to 3 in epoch 1, so a header with one of their KIDs reaches the cipher.
static RECEIVER: LazyLock<Mutex<Receiver>> = LazyLock::new(|| {
    let mut receiver = Receiver::default();
    receiver.add_epoch(&[9; 32], 1, &[0, 1, 2, 3]).unwrap();
    Mutex::new(receiver)
});

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((length, data)) = split(data) else {
        return;
    };
    let (metadata, ciphertext) = data.split_at(usize::from(length).min(data.len()));
    let _ = RECEIVER.lock().unwrap().decrypt(metadata, ciphertext);
});
