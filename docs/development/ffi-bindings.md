# FFI Bindings (UniFFI)

How Swift/Kotlin bindings are (meant to be) produced from the Rust core. The
API itself — exports, errors, ownership, threading, security — is defined only
in [FFI Contract](../architecture/ffi-design.md); this page does not repeat it.

> ⚠️ **Status (2026-10-06): bindings cannot be generated.** The crate builds,
> but `uniffi-bindgen` rejects the `Result<T, String>` error type used by every
> fallible export. No generated Swift/Kotlin files exist in the repository or
> in any build output.

## Inputs

| Item | Value |
|------|-------|
| Crate | `rust/ffi` (package `dyapp-ffi`, lib `dyapp_ffi`, `crate-type = ["cdylib", "rlib"]`) |
| UniFFI | `0.24.3` (`Cargo.lock`), features `cli` (runtime) and `build` (build-dependency) |
| Interface | proc macros in `rust/ffi/src/lib.rs`; `rust/ffi/src/dyapp.udl` holds only `namespace dyapp {};` |
| Scaffolding | `rust/ffi/build.rs` runs `uniffi::generate_scaffolding("src/dyapp.udl")` |

`cargo build` produces only the native library and Rust scaffolding; it never
writes Swift or Kotlin.

## Generator

The workspace has **no `uniffi-bindgen` binary target**, so
`cargo run --bin uniffi-bindgen …` (used by `tools/build-scripts/build-uniffi.sh`)
fails. The `cli` feature only provides
`uniffi::uniffi_bindgen_main()`; a binary must call it. The version must match
the crate's UniFFI (0.24.3).

The 0.24.3 generator supports `--language kotlin | swift | python | ruby`.
There is **no `c` language** and no `--check` flag.

### Reproduction used to check this page

Run in the `dyapp:network-test` image (offline, crates pre-fetched) with a
throwaway crate outside the repository:

```toml
# Cargo.toml of a scratch crate
[package]
name = "ub"
version = "0.1.0"
edition = "2021"
[workspace]
[dependencies]
uniffi = { version = "=0.24.3", features = ["cli"] }
```

```rust
// src/main.rs
fn main() { uniffi::uniffi_bindgen_main() }
```

```bash
cargo build -p dyapp-ffi --offline          # from a copy of the repo
# from the repository root, with the scratch binary:
ub generate rust/ffi/src/dyapp.udl \
   --lib-file target/debug/libdyapp_ffi.so \
   --language swift --out-dir out/swift
```

Result for both `swift` and `kotlin`:

```text
unsupported error type Some(String)
```

Library mode (`ub generate --library target/debug/libdyapp_ffi.so …`)
fails before that with `cargo metadata returned 0 packages for crate name
dyapp_ffi`.

## `tools/build-scripts/build-uniffi.sh`

**Do not treat a successful run as proof of bindings.** The script:

- calls `cargo run --bin uniffi-bindgen generate src/ffi.rs` (nonexistent
  binary and input), catches the failure, prints a warning and continues;
- overwrites `tools/xcode-uniffi-build.sh`, `android/build-uniffi.gradle` and a
  root `UNIFFI_SETUP.md` with templates that build the nonexistent package
  `dyapp-core` and describe a nonexistent `src/ffi.rs` API with UniFFI 0.25;
- prints `✓ Verification OK` and `✅ UniFFI binding setup complete!` without
  checking for any output (read from the source; the script was not run, since
  it writes into the repository).

It should exit nonzero when bindings are missing and never write sources.

## Pipeline (target)

None of these stages exist yet. Each produces build outputs, never committed
sources:

| Stage | Input | Output | Tool |
|-------|-------|--------|------|
| 1. Native library | `rust/ffi` | `.so` per Android ABI; `libdyapp_ffi.a` for iOS/macOS once `staticlib` is added to `crate-type` (today only `cdylib`/`rlib`) | `cargo build -p dyapp-ffi --release --target <triple>` (needs the target installed; Android also needs an NDK linker) |
| 2. Language bindings | UDL + built library | Swift: `dyapp.swift`, `dyappFFI.h`, `dyappFFI.modulemap`; Kotlin: `uniffi/dyapp/dyapp.kt` (names are UniFFI 0.24 defaults for namespace `dyapp`, unverified here because stage 2 fails) | version-matched `uniffi-bindgen` |
| 3a. Apple package | static libs per platform + header/modulemap | `.xcframework`, wrapped in a Swift package | `lipo` only to merge architectures of the **same** platform (e.g. simulator arm64+x86_64); `xcodebuild -create-xcframework` to combine device, simulator and macOS slices |
| 3b. Android package | `.so` per ABI + Kotlin file | AAR with `jniLibs/<abi>/libdyapp_ffi.so`; Kotlin bindings need JNA at runtime | Gradle (Android library module) |
| 3c. C / .NET | — | needs a hand-written `extern "C"` layer; UniFFI's C symbols are internal and the Swift `…FFI.h` shim is not a public C API | not designed |

Drift validation (target): regenerate in CI and fail if the output differs from
the expected checksum, or compile a consumer against freshly generated
bindings. Nothing does this today.

## What is not implemented

- A bindgen binary or pinned external generator.
- Typed `uniffi::Error` errors (prerequisite for any generated output).
- Committed or CI-checked generated bindings; there is no drift check.
- Packaging: XCFramework/Swift package, Android AAR/`jniLibs`, and a C header
  for Linux/Windows. UniFFI does not produce a public C API; the native symbols
  are UniFFI-internal (see the contract).
- Xcode build phase or Gradle task. `tools/xcode-uniffi-build.sh` and
  `android/build-uniffi.gradle` do not exist.
- FFI tests in Swift or Kotlin. Existing "bridge" classes are stubs that do not
  call Rust.
