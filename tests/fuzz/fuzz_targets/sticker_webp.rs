#![no_main]
//! First-frame decoder: caught panics must still become saved fuzz regressions.

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    match dyapp_client::sticker_webp::decode::first_frame(data) {
        Ok(frame) => {
            assert!(data.len() <= 256 * 1024);
            assert!((1..=512).contains(&frame.width));
            assert!((1..=512).contains(&frame.height));
            assert_eq!(
                frame.pixels.len(),
                frame.width as usize * frame.height as usize * 4
            );
            assert!(frame
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0));
        }
        Err(reason) => assert_ne!(reason, "WebP decoder panic"),
    }
});
