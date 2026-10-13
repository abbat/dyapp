# 0002. Rust core with native mobile apps and Tauri desktop

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

The app targets iOS, Android, macOS, Linux and Windows. Sync, P2P networking, messaging and video
are the hard parts and must behave identically on every platform. The repository has a Rust
workspace (`rust/identity`, `profile`, `messaging`, `p2p-net`, `video`, `bootstrap`, `ffi`) with a UniFFI
surface, native Swift and Kotlin projects, and Tauri projects for the desktop shells (see
[Architecture overview](../architecture/overview.md)).

## Decision

All protocol and data logic lives in one Rust core, split into crates by feature. Platforms call it
through FFI (UniFFI for Swift and Kotlin); the Tauri shell calls the core crates directly from
`#[tauri::command]` functions. UI: SwiftUI on iOS and macOS, Jetpack Compose on
Android, Tauri on Linux and Windows.

## Consequences

- One protocol implementation to test; the FFI boundary becomes the main integration risk (no app
  only the desktop shell links a core crate so far).
- Desktop UI is web technology inside Tauri; macOS stays native.
