# 0002. Rust core with native mobile apps and Tauri desktop

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

The app targets iOS, Android, macOS, Linux and Windows. Sync, P2P networking, messaging and video
are the hard parts and must behave identically on every platform. The repository has a Rust
workspace (`rust/identity`, `profile`, `messaging`, `p2p-net`, `video`, `bootstrap`, `ffi`) with a UniFFI
surface, native Swift and Kotlin projects, and Tauri projects for the desktop shells (see
[Architecture overview](../architecture/overview.md)). Peer supplied JPEG previews also cross the
FFI boundary. Fuzzing found a 479-byte input that panics in the AVX2 path of `zune-jpeg` 0.5.15,
which `image` uses for decoding.

## Decision

All protocol and data logic lives in one Rust core, split into crates by feature. Platforms call it
through FFI (UniFFI for Swift and Kotlin); the Tauri shell calls the core crates directly from
`#[tauri::command]` functions. UI: SwiftUI on iOS and macOS, Jetpack Compose on
Android, Tauri on Linux and Windows.

The Rust FFI decodes incoming previews with `jpeg-decoder` 0.3.2 while `image` remains the encoder
and resizer. The decoder is bounded to 1 MiB; only RGB and grayscale output is accepted after
checking the input size and frame dimensions. The crash input remains in the fuzz corpus.

## Consequences

- One protocol implementation to test; the FFI boundary becomes the main integration risk (no app
  only the desktop shell links a core crate so far).
- Desktop UI is web technology inside Tauri; macOS stays native.
- Preview handling gains one direct Rust dependency and two JPEG implementations. CMYK and 16-bit
  previews are rejected; app-generated previews are RGB. Fuzzing continues to guard the decoder.
