# Desktop video frame rendering spike

> **Status:** a synthetic Linux debug-build source exists; the Canvas consumer and measurements
> are planned. No platform app renders call media yet. No latency or CPU result has been measured.

[The video core](../architecture/video.md#what-exists-today) has offer/answer and connection
state, but no capture, media track or decoded-frame callback. Linux and Windows desktop apps do
not depend on `dyapp-video` or expose a video Tauri command. A synthetic frame source can test
the desktop rendering boundary without mistaking it for an end-to-end call.

## First candidate

Tauri 2 on Linux uses WebKit2GTK 4.1 ([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/));
the Debian 12 package currently supplies `libwebkit2gtk-4.1-0`
([Debian package](https://packages.debian.org/bookworm/libwebkit2gtk-4.1-0)).
Keep WebRTC transport and decoding in Rust, as [ADR 0013](../decisions/0013-video-calls-one-to-one.md)
requires. WebKitGTK WebRTC availability changes by release: its 2.54 release disabled the
previous experimental backend while a replacement is developed
([WebKitGTK release notes](https://webkitgtk.org/2026/09/16/webkitgtk-2.54-highlights.html)).
No rendering choice should depend on WebView WebRTC being present.

The Linux debug build now has a `ui_test_video_frames` command, guarded at runtime by
`--ui-test`. It accepts `360p` or `720p` and 1–1800 frames, sends at 30 fps from a blocking
worker, and uses Tauri's binary `Channel<InvokeResponseBody>::Raw`. Each frame has a 20-byte
little-endian header: `DVF1`, width and height as `u16`, sequence as `u32`, and Unix send time
in microseconds as `u64`, followed by packed RGBA bytes. The source reuses one pixel template
and copies it per frame. Release builds do not register the command. The next checkpoint will
receive the bytes in JavaScript as `ArrayBuffer` and paint one persistent Canvas 2D surface
with `ImageData` and `putImageData`. Tauri documents that `Channel<Vec<u8>>` serializes bytes
as a JSON array, so the benchmark uses the raw form
([Tauri Channel API](https://docs.rs/tauri/latest/tauri/ipc/struct.Channel.html)).
A bounded queue keeps at most one pending frame; when the renderer falls behind, it drops the
older frame rather than growing memory or showing stale video. Frame payloads carry width,
height, sequence and sender timestamp; malformed dimensions and oversized payloads are refused.
This is a rendering experiment, not a replacement for SFrame or the media transport.

## Measurements and decision gate

Run on a real Debian 12 desktop with the distribution WebKitGTK package; record package version,
CPU/GPU, display backend, frame size, sample duration and whether acceleration is available.
Xvfb CI may check that the path still renders, but its software display is not a performance
result. Use the same 60-second synthetic source for an idle baseline and for each case:

| Case | Raw payload at 30 fps | Purpose |
|---|---:|---|
| 640 × 360 RGBA | 0.92 MB/frame, 27.6 MB/s | Lower resolution call |
| 1280 × 720 RGBA | 3.69 MB/frame, 110.6 MB/s | Desired desktop baseline |

Record produced, delivered, painted and dropped frames, p50/p95 age from Rust send to the next
paint opportunity, and CPU time for both the app and its WebKit child processes relative to idle.
Use a shared host clock sample to check timestamp offset; report the uncertainty instead of
calling a round-trip estimate one-way latency. Observe memory growth after warm-up and window
resize. The proposed pass gate for 720p30 is p95 frame age below 100 ms, under 5% dropped
frames and under one additional CPU core on the reference desktop. These are provisional
product targets, not measurements or an approved minimum device specification. A slower or
software-only machine is reported separately.

If the raw path misses the gate, first measure a compressed-frame path or a native renderer
rather than adding unmeasured buffering. A GTK/GStreamer surface may avoid some WebView copies
when the decoder can hand it native buffers, but adds Linux-specific window/overlay work; the
actual zero-copy behavior must be demonstrated. If the raw path passes, repeat with real decoded
frames when the Rust media track exists and include the SFrame and decode costs. The first
benchmark does not prove real-call performance.

## Trade-offs

| Route | Benefit | Cost or uncertainty |
|---|---|---|
| Raw Tauri channel to Canvas 2D | Uses the existing Tauri window and a documented binary channel; small first experiment. | 720p30 sends about 111 MB/s before copies and IPC overhead; hardware behavior must be measured. |
| Encoded frames to WebView media element | Much smaller IPC payload if a supported decode path exists. | Codec/container/MSE support and buffering vary by installed WebKitGTK and GStreamer plugins. |
| Native GTK/GStreamer surface | May reduce copies and give control over video timing. | Separate Linux integration and overlay/focus behavior; harder to share with Windows. |

Next checkpoint: add the Canvas consumer, bounded metrics and Linux UI smoke for this source;
then record measurements on a named Debian 12 host before choosing the production renderer.
