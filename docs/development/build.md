# Build Scripts (Cross-Compilation)

> **Status:** the scripts in `tools/build-scripts/` cross-compile Rust crates,
> but **none produces a library an app can load**. They do not build the FFI
> crate, do not create an XCFramework or AAR, and do not produce a web module.
> Cross-compiled crates are not an installable application. This page states
> what each script actually does; the packaging that is missing is in
> [FFI bindings](ffi-bindings.md). The apps do not use these scripts: the Android Gradle
> build compiles the core itself ([FFI bindings → Android](ffi-bindings.md#android)), and the
> iOS and macOS apps link the XCFramework from `scripts/build-apple-core.sh`
> ([FFI bindings → Apple](ffi-bindings.md#apple)).
>
> Everything below is read from the script sources. The scripts were **not
> run** for this page: they need Xcode or the Android NDK, and they write files
> into the repository.

## Environment

| Item | Value | Source |
|------|-------|--------|
| Rust toolchain | `1.99.0` (pinned), components `rustfmt`, `clippy` | `rust-toolchain.toml` |
| MSRV | `1.99` | `rust-version` in root `Cargo.toml` |
| Swift | language mode 5 (`SWIFT_VERSION`) | `ios/project.yml`, `macos/project.yml` |
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
`libdyapp_core.so`, which no crate produces, and no JNI symbols such as
`nativeInitialize` exist on the Rust side.

### `build-uniffi.sh`

Fails to generate bindings and still reports success. See
[FFI bindings](ffi-bindings.md#toolsbuild-scriptsbuild-uniffish).

## What a working build would need (roadmap)

Not implemented. This is the order in which the gaps block each other:

1. Build `dyapp-ffi` instead of the four internal crates.
2. **iOS and macOS:** done in `scripts/build-apple-core.sh` (Swift bindings and an
   XCFramework, see [FFI bindings → Apple](ffi-bindings.md#apple)); release builds,
   signing and an App Store archive are not.
3. **Android:** done in `android/app/build.gradle` (`cargo ndk` for arm64-v8a and
   x86_64, Kotlin bindings, JNA); release signing and an AAB are not.
4. **Web:** needs its own `wasm-bindgen` crate and a decision on which core
   crates can target `wasm32` (sockets and `sled` storage cannot). There is no
   such crate today.
5. CI: there is no build-matrix workflow. `.github/workflows/` has no
   cross-compile job.

## Troubleshooting

| Symptom | Cause |
|---------|-------|
| iOS script says "XCFramework created", but there is nothing in `frameworks/` except `Modules/` | Expected: the step is a placeholder |
| Android script ends with "No libraries found in build output" | Expected: the built crates are rlib-only |
| `ANDROID_NDK_HOME not set` | `export ANDROID_NDK_HOME=<sdk>/ndk/<version>` |
