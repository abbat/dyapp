# FFI Bindings (UniFFI)

How Swift/Kotlin bindings are produced from the Rust core. The API itself —
exports, errors, ownership, threading, security — is defined only in
[FFI Contract](../architecture/ffi-design.md); this page does not repeat it.

> **Status:** the Android app generates Kotlin bindings and builds the core for
> each ABI during its Gradle build; `tools/scripts/build-apple-core.sh` generates Swift
> bindings and an XCFramework for iOS and macOS. Generated files are build outputs
> and are never committed.

## Inputs

| Item | Value |
|------|-------|
| Crate | `rust/ffi` (package `dyapp-ffi`, lib `dyapp_ffi`, `crate-type = ["cdylib", "rlib"]`) |
| UniFFI | `0.32.2` (`Cargo.lock`), features `cli` (runtime) and `build` (build-dependency) |
| Interface | proc macros in `rust/ffi/src/lib.rs`; `rust/ffi/src/dyapp.udl` holds only `namespace dyapp {};` |
| Scaffolding | `rust/ffi/build.rs` runs `uniffi::generate_scaffolding("src/dyapp.udl")` |
| Generator | `rust/ffi/src/bin/uniffi-bindgen.rs`, a bin target calling `uniffi::uniffi_bindgen_main()`, so the generator always matches the crate's UniFFI |

## Generating bindings

The generator runs in library mode: it reads the interface from a host build of
`libdyapp_ffi.so`. Build the whole package first: `cargo run` of the bin alone
does not produce the cdylib.

```bash
cargo build --locked -p dyapp-ffi
build/debug/uniffi-bindgen generate \
    --library build/debug/libdyapp_ffi.so --language kotlin --no-format --out-dir <dir>
```

Kotlin output is one file, `uniffi/dyapp/dyapp.kt` (package `uniffi.dyapp`). It
loads `libdyapp_ffi.so` through JNA (`net.java.dev.jna:jna`, the `@aar` artifact
on Android) and throws `CoreException` for a failed call.

On Android, `DYApplication.onCreate` also loads the same library through `System.loadLibrary`
and calls the JNI export `Java_com_dyapp_DYApplication_nativeInit` (`rust/ffi/src/android.rs`). It stores the Java VM and
the application `Context` in `ndk-context` once per process, so the libp2p DNS resolver can read
the system name servers ([ADR 0032](../decisions/0032-android-context-for-dns.md)).

## Android

`clients/android/app/build.gradle` wires both steps into the app build; outputs land in
`build/android/app/rust`:

| Task | Runs | Output |
|------|------|--------|
| `cargoHost`, `uniffiBindgen` | the two commands above | `build/rust/kotlin`, added as a Kotlin source directory |
| `cargoNdkDebug` / `cargoNdkRelease` | `cargo ndk --platform 31 -t arm64-v8a -t x86_64 -o … build --locked -p dyapp-ffi --lib` (`--release` for release; debug builds drop debuginfo) | `build/rust/<buildType>/jniLibs/<abi>/libdyapp_ffi.so` |

Both run before `preDebugBuild`/`preReleaseBuild`. They need Rust 1.99.0 with the
`aarch64-linux-android` and `x86_64-linux-android` targets, `cargo-ndk` 4.1.2,
the NDK (`ANDROID_NDK_HOME`, r29 `29.0.14206865`) and a host C toolchain;
`tests/docker/Dockerfile.android-test` installs all of them. The property
`-Pdyapp.prebuiltRust` skips both tasks and reuses `build/rust` from an earlier
build; the Docker unit-test run uses it because it has no Rust sources.

## Apple

`tools/scripts/build-apple-core.sh [ios|ios-device|macos|all] [debug|release]` runs
on macOS with Xcode. The defaults remain `all debug`; `ios-device` builds only
the device slice. `release` optimizes the target static libraries while the host
binding generator stays in the dev profile. Outputs land in `build/apple`:

| Step | Runs | Output |
|------|------|--------|
| Bindings | `cargo build --locked -p dyapp-ffi --target <host>`, then `uniffi-bindgen generate --library …/libdyapp_ffi.dylib --language swift` | `swift/dyapp.swift`; `headers/dyappFFI.h` and `headers/module.modulemap` (module `dyappFFI`) |
| Static libraries | `cargo rustc <profile flags> --locked -p dyapp-ffi --lib --crate-type staticlib --target <t>` per target, so `crate-type` stays `cdylib`/`rlib` for the other builds | `build/<t>/<debug-or-release>/libdyapp_ffi.a` |
| Slices | `lipo` merges the architectures of one platform: `ios` = `aarch64-apple-ios`; `ios-simulator` = `aarch64-apple-ios-sim` + `x86_64-apple-ios`; `macos` = `aarch64-apple-darwin` + `x86_64-apple-darwin` | `<slice>/libdyapp_ffi.a` |
| XCFramework | `xcodebuild -create-xcframework` with one `-library`/`-headers` pair per slice | `DyappFFI.xcframework` |

The default libraries are unoptimised dev builds without debuginfo; `release`
uses the workspace release profile. Both target iOS 15 and macOS 12 (`IPHONEOS_DEPLOYMENT_TARGET`, `MACOSX_DEPLOYMENT_TARGET`), the apps'
deployment targets. `clients/ios/project.yml` and `clients/macos/project.yml` compile
`build/apple/swift/dyapp.swift` into the app and link the XCFramework without
embedding it, so run the script before XcodeGen. There is no Xcode build phase:
Xcode resolves an XCFramework before any script phase could build it.

## Linux, Windows

The Tauri apps call the core crates directly and need no bindings. UniFFI does
not produce a public C API; the native symbols are UniFFI-internal (see the
contract).

## `tools/build-scripts/build-uniffi.sh`

**Do not treat a successful run as proof of bindings.** The script:

- calls `cargo run --bin uniffi-bindgen generate src/ffi.rs` (wrong input),
  catches the failure, prints a warning and continues;
- overwrites `tools/xcode-uniffi-build.sh`, `clients/android/build-uniffi.gradle` and a
  root `UNIFFI_SETUP.md` with templates that build the nonexistent package
  `dyapp-core` and describe a nonexistent `src/ffi.rs` API with UniFFI 0.25;
- prints `✓ Verification OK` and `✅ UniFFI binding setup complete!` without
  checking for any output (read from the source; the script was not run, since
  it writes into the repository).

It should exit nonzero when bindings are missing and never write sources. The
Android build does not use it.

## What is not implemented

- Signed/notarized Apple release distribution; the release producer measures unsigned CI packages.
- A drift check of the generated interface; the UI tests only call it.
- FFI tests beyond the `dyapp-ffi` unit tests and the Android, iOS and macOS UI tests, which
  check the peer ID the apps show.
