# 0002. Rust core with native mobile apps and Tauri desktop

- **Status:** Accepted
- **Date:** 2026-10-23
- **Revised:** 2026-11-07: macOS moves from SwiftUI to the Tauri desktop client for v0.0.1

## Context

The app targets iOS, Android, macOS, Linux and Windows. Sync, P2P networking, messaging and video
are the hard parts and must behave identically on every platform. The repository has a Rust
workspace (`rust/identity`, `profile`, `messaging`, `p2p-net`, `video`, `bootstrap`, `ffi`) with a UniFFI
surface, native Swift and Kotlin projects, and Tauri projects for the desktop shells (see
[Architecture overview](../architecture/overview.md)). Peer supplied JPEG previews also cross the
FFI boundary. Fuzzing found a 479-byte input that panics in the AVX2 path of `zune-jpeg` 0.5.15,
which `image` uses for decoding.

By November 2026 the shared Tauri desktop source (`clients/desktop`, built by `clients/linux` and
`clients/windows`) was the most complete client, while the SwiftUI macOS app was a small subset
that repeated each desktop screen by hand. No client had shipped, so the split could still change.

## Decision

All protocol and data logic lives in one Rust core, split into crates by feature. Platforms call it
through FFI (UniFFI for Swift and Kotlin); the Tauri shell calls the core crates directly from
`#[tauri::command]` functions. UI: SwiftUI on iOS, Jetpack Compose on Android, and one Tauri
desktop client on Linux, Windows and macOS.

Shared validation, parsing, policy, state machines and storage rules also belong in Rust,
including when only one client currently needs them. Android and iOS call them through UniFFI;
the desktop shell uses the same core APIs. Clients retain rendering, input, navigation and
platform services such as secure key storage, notifications, permissions and file pickers.
Before implementing non-presentation logic in a client, record the concrete platform constraint
or measured need in the task and obtain maintainer agreement; keep the reason beside the code.
This boundary is required for Android v0.0.1 development, rather than a later cleanup.

The macOS client is a third thin crate over `clients/desktop`, like `clients/linux` and
`clients/windows`. It replaces the SwiftUI macOS app before v0.0.1. Platform code in the shared
source is selected by `target_os`; `cfg(unix)` stays only for code that is the same on Linux and
macOS (file modes, `statvfs`). D-Bus notifications, tray-host discovery, Linux
single-instance signaling, NetworkManager metering and WebKitGTK camera permissions
compile only on Linux; other desktops use their platform paths. iOS and Android stay native.

The Rust FFI decodes incoming previews with `jpeg-decoder` 0.3.2 while `image` remains the encoder
and resizer. The decoder is bounded to 1 MiB; only RGB and grayscale output is accepted after
checking the input size and frame dimensions. The crash input remains in the fuzz corpus.

## Consequences

- One protocol implementation to test; the FFI boundary becomes the main integration risk (no app
  only the desktop shell links a core crate so far).
- Desktop UI is web technology inside Tauri on all three desktop systems; one screen change and one
  UI smoke cover them all. macOS loses native AppKit controls and must add what WKWebView does not
  give for free: the app menu with Edit (copy and paste shortcuts), Dock reopen and the menu-bar
  tray.
- macOS gains the desktop features it lacked, including the in-process node
  ([ADR 0030](0030-desktop-full-node.md)), and needs the Keychain store of
  [ADR 0022](0022-desktop-system-credential-storage.md).
- The macOS SwiftUI app, its XCUITests and the macOS slice of the Apple XCFramework go away; the
  Swift code, `build-apple-core.sh` and Apple CI remain for iOS.
- Preview handling gains one direct Rust dependency and two JPEG implementations. CMYK and 16-bit
  previews are rejected; app-generated previews are RGB. Fuzzing continues to guard the decoder.
