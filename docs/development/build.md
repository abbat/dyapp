# Build Scripts (Cross-Compilation)

> **Status:** the scripts in `tools/build-scripts/` cross-compile Rust crates,
> but **none produces a library an app can load**. They do not build the FFI
> crate, do not create an XCFramework or AAR, and do not produce a web module.
> Cross-compiled crates are not an installable application. This page states
> what each script actually does; the packaging that is missing is in
> [FFI bindings → Pipeline (target)](ffi-bindings.md#pipeline-target).
>
> Everything below is read from the script sources. The scripts were **not
> run** for this page: they need Xcode or the Android NDK, and they write files
> into the repository.

## Environment

| Item | Value | Source |
|------|-------|--------|
| Rust toolchain | `1.99.0` (pinned), components `rustfmt`, `clippy` | `rust-toolchain.toml` |
| MSRV | `1.99` | `rust-version` in root `Cargo.toml` |
| Swift tools | 5.9, which needs **Xcode 15+** | `ios/Package.swift`, `macos/Package.swift` |
| iOS / macOS minimum | iOS 14, macOS 11 | same |
| Android app | `minSdk 26`, `compileSdk 37`, `targetSdk 36` | `android/app/build.gradle` |
| Android NDK | any; `ANDROID_NDK_HOME` must be set | `build-android.sh` |

Run every script from any directory: each resolves the repository root from
its own path and builds there.

## What each script does

### `build-all.sh`

`--ios`, `--android`, `--web`; no flag means all three. It calls the two
scripts below. For `--web` it runs
`cargo build --target wasm32-unknown-unknown --release` on the **whole
workspace**. No crate has `wasm-bindgen` exports, so even if that build
succeeded (not verified), the result would be rlibs, not a JavaScript-loadable
module. There is no web frontend in the repository.

### `build-ios.sh`

| Step | What happens |
|------|--------------|
| Prerequisites | Exits if `rustup` or `xcodebuild` is missing |
| Targets | **Installs** `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios` with `rustup target add` if missing |
| Build | `cargo build -p <crate> --target <t> --release --lib` for `dyapp-identity`, `-profile`, `-p2p-net`, `-messaging`, `-video` |
| Headers | Copies `target/uniffi-headers/*` if that directory exists. Nothing creates it |
| Modulemap | Writes `target/ios-build/frameworks/Modules/module.modulemap` for `DYAppCore`, with umbrella header `DYAppCore.h`, which does not exist |
| XCFramework | **Placeholder**: prints "requires manual Xcode setup" followed by "✓ XCFramework created". Nothing is created |

These four crates have no `crate-type`, so they build only as **rlib**, which
Xcode cannot link. `dyapp-ffi`, the only crate with an exported surface,
is not built. The final "✅ iOS build complete!" is printed regardless.

### `build-android.sh`

| Step | What happens |
|------|--------------|
| Prerequisites | Exits if `rustup` is missing or `ANDROID_NDK_HOME` is unset; **runs `cargo install cargo-ndk`** if it is missing |
| Targets | **Installs** `aarch64-linux-android`, `armv7-linux-androideabi` if missing |
| Build | `cargo ndk -t <triple> -o target/android-build/lib -- build -p <same four crates> --release --lib` |
| Organize | Copies `*.so` from `target/android-build/lib/<triple>/` to `target/android-build/gradle/jniLibs/<abi>/` |
| Writes into the repo | `android/build-rust.gradle` and `android/CMakeLists.txt`. Neither is tracked, and `android/app` does not apply either |
| Verify | Exits 1 with "No libraries found" when `jniLibs` has no `.so` |

The four crates are rlib-only, so there are no `.so` files to copy, and
`verify_build` is expected to fail. The generated CMake file imports
`libdyapp_core.so`, which no crate produces. `android/app/.../RustBridge.kt`
has `System.loadLibrary` commented out, and no JNI symbols such as
`nativeInitialize` exist on the Rust side.

### `build-uniffi.sh`

Fails to generate bindings and still reports success. See
[FFI bindings](ffi-bindings.md#toolsbuild-scriptsbuild-uniffish).

## What a working build would need (roadmap)

Not implemented. This is the order in which the gaps block each other:

1. Build `dyapp-ffi` instead of the four internal crates. Add
   `staticlib` to its `crate-type` for iOS.
2. Generate bindings ([blocked](ffi-bindings.md) by `Result<_, String>` errors).
3. **iOS:** package per-platform static libraries with
   `xcodebuild -create-xcframework`. Device (`aarch64-apple-ios`) and simulator
   (`aarch64-apple-ios-sim` + `x86_64-apple-ios`, merged with `lipo` because they
   are the same platform) are separate slices. `lipo` alone cannot create an
   XCFramework.
4. **Android:** `cargo ndk` for the FFI `cdylib`, put the `.so` files in
   `jniLibs/<abi>`, and ship the generated Kotlin with a JNA dependency. UniFFI
   does not use hand-written JNI functions.
5. **Web:** needs its own `wasm-bindgen` crate and a decision on which core
   crates can target `wasm32` (sockets and `sled` storage cannot). There is no
   such crate today.
6. CI: there is no build-matrix workflow. `.github/workflows/` has no
   cross-compile job.

## Troubleshooting

| Symptom | Cause |
|---------|-------|
| iOS script says "XCFramework created", but there is nothing in `frameworks/` except `Modules/` | Expected: the step is a placeholder |
| Android script ends with "No libraries found in build output" | Expected: the built crates are rlib-only |
| `ANDROID_NDK_HOME not set` | `export ANDROID_NDK_HOME=<sdk>/ndk/<version>` |
| Xcode refuses `ios/Package.swift` | Swift tools 5.9 needs Xcode 15 or newer |
