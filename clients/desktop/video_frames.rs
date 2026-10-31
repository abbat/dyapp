//! Synthetic RGBA source for the Linux WebView rendering experiment.
//! Wire format: 20-byte little-endian header (DVF1, width, height, sequence,
//! Unix microseconds at send) followed by tightly packed RGBA pixels.
//! This module is compiled only into Linux debug builds.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::ipc::{Channel, InvokeResponseBody};

const HEADER_LEN: usize = 20;
const MAX_FRAMES: u16 = 1800;
const FRAME_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 30);
static RUNNING: AtomicBool = AtomicBool::new(false);

struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

fn dimensions(size: &str) -> Result<(u16, u16), String> {
    match size {
        "360p" => Ok((640, 360)),
        "720p" => Ok((1280, 720)),
        _ => Err("Unsupported video test size".into()),
    }
}

fn template(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0; HEADER_LEN + usize::from(width) * usize::from(height) * 4];
    bytes[..4].copy_from_slice(b"DVF1");
    bytes[4..6].copy_from_slice(&width.to_le_bytes());
    bytes[6..8].copy_from_slice(&height.to_le_bytes());
    for pixel in bytes[HEADER_LEN..].chunks_exact_mut(4) {
        pixel.copy_from_slice(&[32, 96, 160, 255]);
    }
    bytes
}

fn frame(base: &[u8], sequence: u32, sent_at_us: u64) -> Vec<u8> {
    let mut bytes = base.to_vec();
    bytes[8..12].copy_from_slice(&sequence.to_le_bytes());
    bytes[12..20].copy_from_slice(&sent_at_us.to_le_bytes());
    bytes
}

fn send_frames(
    size: &str,
    count: u16,
    channel: Channel<InvokeResponseBody>,
) -> Result<u16, String> {
    let (width, height) = dimensions(size)?;
    if !(1..=MAX_FRAMES).contains(&count) {
        return Err("Video test frame count is out of range".into());
    }
    let base = template(width, height);
    let started = Instant::now();
    for sequence in 0..u32::from(count) {
        let target = FRAME_PERIOD.saturating_mul(sequence);
        if let Some(wait) = target.checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
        let sent_at_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_micros() as u64;
        channel
            .send(InvokeResponseBody::Raw(frame(&base, sequence, sent_at_us)))
            .map_err(|error| error.to_string())?;
    }
    Ok(count)
}

#[tauri::command]
pub(crate) async fn ui_test_video_frames(
    size: String,
    count: u16,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<u16, String> {
    if !std::env::args().any(|arg| arg == "--ui-test") {
        return Err("Video source is available only in UI tests".into());
    }
    if RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_err()
    {
        return Err("Video test source is already running".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = RunningGuard;
        send_frames(&size, count, on_frame)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_have_bounded_rgba_payload_and_little_endian_header() {
        for (size, width, height) in [("360p", 640, 360), ("720p", 1280, 720)] {
            let (w, h) = dimensions(size).unwrap();
            assert_eq!((w, h), (width, height));
            let bytes = frame(&template(w, h), 0x12345678, 42);
            assert_eq!(
                bytes.len(),
                HEADER_LEN + usize::from(w) * usize::from(h) * 4
            );
            assert_eq!(&bytes[..4], b"DVF1");
            assert_eq!(u16::from_le_bytes(bytes[4..6].try_into().unwrap()), w);
            assert_eq!(u16::from_le_bytes(bytes[6..8].try_into().unwrap()), h);
            assert_eq!(
                u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
                0x12345678
            );
            assert_eq!(u64::from_le_bytes(bytes[12..20].try_into().unwrap()), 42);
            assert_eq!(&bytes[20..24], &[32, 96, 160, 255]);
        }
        assert!(dimensions("1080p").is_err());
    }

    #[test]
    fn channel_sends_raw_bytes_once() {
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let frames = received.clone();
        let channel = Channel::new(move |message| {
            match message {
                InvokeResponseBody::Raw(bytes) => frames.lock().unwrap().push(bytes),
                InvokeResponseBody::Json(_) => panic!("synthetic frame became JSON"),
            }
            Ok(())
        });
        assert_eq!(send_frames("360p", 1, channel).unwrap(), 1);
        let frames = received.lock().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(&frames[0][..4], b"DVF1");
    }

    #[test]
    fn count_limits_are_enforced_before_sending() {
        let channel = Channel::new(|_| Ok(()));
        assert!(send_frames("360p", 0, channel.clone()).is_err());
        assert!(send_frames("360p", MAX_FRAMES + 1, channel.clone()).is_err());
        assert!(send_frames("invalid", 1, channel).is_err());
    }
}
