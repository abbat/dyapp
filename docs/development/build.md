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

## Unsigned Android Release

Inside the prepared Android image, run
`bash scripts/build-android-release.sh /tmp/ai/release-reports` from a writable
copy of this checkout. The copy needs `Cargo.toml`, `Cargo.lock`, `rust/`,
`proto/`, `android/` and `scripts/`; use a writable copy of the image's prepared
Gradle cache as `GRADLE_USER_HOME`. The existing SDK, NDK, Cargo dependencies and
Gradle dependencies must already be available: the runner uses offline mode and
installs nothing. It requests `assembleRelease` and `bundleRelease`, including
release Rust JNI libraries; it does not set `dyapp.prebuiltRust`.

The existing prepared image's offline probe on 2026-10-15 reached Release lint,
but failed because `com.android.tools.lint:lint-gradle:32.4.1` was not cached.
Rust JNI and Kotlin/Java compilation completed; end-to-end packaging is not yet
verified. Manual cache/image preparation is required before retrying; the runner
does not bypass `lintVitalAnalyzeRelease` or fetch the missing dependency.

`build.log` retains Gradle output. On success `packages.json` records APK/AAB
bytes, SHA-256 and the required arm64-v8a/x86_64 entries. The verifier requires
nonempty manifest/dex/library members, rejects duplicate ZIP names and checks
64-bit little-endian ELF machine identifiers for the native libraries. This is
structural verification, not a signature, full executable or installation test.
The runner expects `app-release-unsigned.apk` and `app-release.aab`; signing and
store distribution remain planned. Scheduled production and matching Android
size budgets are also planned; the global D5 practice is not yet adopted.

## Unsigned Apple Release packages

`scripts/package-apple-release.py <ios|macos> <DYApp.app> <new-archive>`
verifies bundle structure and packages an app in approved native Apple CI. Its CLI
uses `xcrun lipo` and requires `GITHUB_ACTIONS=true`; pure packaging tests run in
Docker with fixture architecture lists. iOS requires platform `iphoneos` and
arm64; macOS requires `macosx` with both arm64 and x86_64. The executable must be
a nonempty file with owner execute permission, named by a basename in the
platform's generated `Info.plist`.

IPA members live under `Payload/DYApp.app/`; macOS ZIP members under `DYApp.app/`.
Member order and timestamps are normalized; executable modes and internal
symlinks are preserved. Links outside the app, archive paths inside it and
existing archive outputs are rejected. Package structure checks cover platform,
architecture and filesystem layout. Native Release producers, actual package
measurements, size gates, signing and notarization remain planned.

## What a working build would need (roadmap)

Not implemented. This is the order in which the gaps block each other:

1. Build `dyapp-ffi` instead of the four internal crates.
2. **iOS and macOS:** done in `scripts/build-apple-core.sh` (Swift bindings and an
   XCFramework, see [FFI bindings → Apple](ffi-bindings.md#apple)); release builds,
   signing and an App Store archive are not.
3. **Android:** done in `android/app/build.gradle` (`cargo ndk` for arm64-v8a and
   x86_64, Kotlin bindings, JNA). The [offline Release runner](#unsigned-android-release)
   requests unsigned APK/AAB; missing offline Release lint dependencies currently
   block verified packaging. Release signing and store distribution remain planned.
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
