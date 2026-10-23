# Build Scripts (Cross-Compilation)

> **Status:** the scripts in `tools/build-scripts/` cross-compile Rust crates,
> but **none produces a library an app can load**. They do not build the FFI
> crate, do not create an XCFramework or AAR, and do not produce a web module.
> Cross-compiled crates are not an installable application. This page states
> what each script actually does; the packaging that is missing is in
> [FFI bindings](ffi-bindings.md). The apps do not use these scripts: the Android Gradle
> build compiles the core itself ([FFI bindings → Android](ffi-bindings.md#android)), and the
> iOS and macOS apps link the XCFramework from `tools/scripts/build-apple-core.sh`
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
| Swift | language mode 6, strict concurrency (`SWIFT_VERSION`) | `clients/ios/project.yml`, `clients/macos/project.yml` |
| iOS / macOS minimum | iOS 14, macOS 11 | same |
| Android app | `minSdk 26`, `compileSdk 37`, `targetSdk 36` | `clients/android/app/build.gradle` |
| Android NDK | any; `ANDROID_NDK_HOME` must be set | `build-android.sh` |

Run every script from any directory: each resolves the repository root from
its own path and builds there.

## Local build and packaging

`make build` runs `tools/scripts/build-rust.sh` in the shared development image.
`make deb` runs `tools/scripts/build-deb.sh` in the Debian 12 packaging image,
then runs package-install tests from `tests/scripts/`. On a version tag, the
release workflow requires the Linux client version to match the tag, collects
`dyapp_X.Y.Z_amd64.deb`, includes it in `SHA256SUMS`, attests its provenance and
publishes it with the server package. A manual workflow run only builds artifacts.
`tools/docker/Dockerfile.linux-base` holds the shared Linux client build
requirements. `tests/docker/Dockerfile.linux-test` adds Xvfb and test binaries;
the packaging image uses the shared base without test-only packages.
`make prepare` builds the base before the Linux test and packaging images.

## Client update feed generator

`python3 tools/scripts/build-update-feed.py INVENTORY.json [PREVIOUS.json]`
prints a deterministic schema 1 stable feed. The inventory is trusted release-CI
input, and the optional previous feed must already come from the fixed trusted
origin. The script checks structure, asset names and digests, supported methods,
version order and rollback; it does not fetch releases, verify attestations or
publish Pages. See [client updates](../design/client-updates.md#offline-feed-generator-checkpoint)
for the inventory contract and current platform limits. Release-CI integration
is still planned.

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
| Headers | Copies `build/uniffi-headers/*` if that directory exists. Nothing creates it |
| Modulemap | Writes `build/ios-build/frameworks/Modules/module.modulemap` for `DYAppCore`, with umbrella header `DYAppCore.h`, which does not exist |
| XCFramework | **Placeholder**: prints "requires manual Xcode setup" followed by "✓ XCFramework created". Nothing is created |

These four crates have no `crate-type`, so they build only as **rlib**, which
Xcode cannot link. `dyapp-ffi`, the only crate with an exported surface,
is not built. The final "✅ iOS build complete!" is printed regardless.

### `build-android.sh`

| Step | What happens |
|------|--------------|
| Prerequisites | Exits if `rustup` is missing or `ANDROID_NDK_HOME` is unset; **runs `cargo install cargo-ndk`** if it is missing |
| Targets | **Installs** `aarch64-linux-android`, `armv7-linux-androideabi` if missing |
| Build | `cargo ndk -t <triple> -o build/android-build/lib -- build -p <same four crates> --release --lib` |
| Organize | Copies `*.so` from `build/android-build/lib/<triple>/` to `build/android-build/gradle/jniLibs/<abi>/` |
| Writes into the repo | `clients/android/build-rust.gradle` and `clients/android/CMakeLists.txt`. Neither is tracked, and `clients/android/app` does not apply either |
| Verify | Exits 1 with "No libraries found" when `jniLibs` has no `.so` |

The four crates are rlib-only, so there are no `.so` files to copy, and
`verify_build` is expected to fail. The generated CMake file imports
`libdyapp_core.so`, which no crate produces, and no JNI symbols such as
`nativeInitialize` exist on the Rust side.

### `build-uniffi.sh`

Fails to generate bindings and still reports success. See
[FFI bindings](ffi-bindings.md#toolsbuild-scriptsbuild-uniffish).

## Gradle dependency verification

`clients/android/gradle/verification-metadata.xml` lists the SHA-256 of every Gradle artifact and
metadata file the Android build resolves: plugins, libraries, lint and test dependencies. Gradle
refuses an artifact that is missing from the file or whose checksum differs, so a tampered or
unexpected artifact fails the `dyapp:android-test` image build. Signatures are not checked.
The root `settings.gradle` substitutes fixed versions for vulnerable AGP build-classpath and
project configurations, including the separate release lint tool, before Gradle reads their
metadata. Its substitutions and this checksum inventory
must change together. The 2026-10-23 update replaced commons-lang3, jose4j, Bouncy Castle and
jdom2, and removed old httpclient and Kotlin plugin entries. Check `buildEnvironment`, Android
tests and Scorecard after any substitution change; these packages are not in the app runtime.

Dependabot does not update the file, so a Gradle update PR fails the Android build until the file is
regenerated on its branch. Delete the file, run the image build's tasks online in the current image
with an empty Gradle cache (a cached artifact skips the BOMs and parent POMs that a fresh download
reads) and the branch's `clients/android` mounted, then review the changed entries in the diff:

```bash
rm clients/android/gradle/verification-metadata.xml
docker run --rm --user 1000:1000 -e GRADLE_USER_HOME=/tmp/gradle \
  -v "$PWD/clients/android:/app/clients/android" \
  dyapp:android-test bash -c 'cd clients/android && ./gradlew --no-daemon \
  --write-verification-metadata sha256 :app:assembleDebug :app:assembleDebugAndroidTest \
  :app:compileDebugUnitTestKotlin :app:testDebugUnitTest :app:assembleRelease :app:bundleRelease \
  -x cargoNdkRelease -x cargoNdkDebug -x cargoHost -x uniffiBindgen'
```

## Unsigned Android Release

`build-android-release.sh` also wraps argument/startup failures with mapped
results; `--raw` bypasses this frontend and `--help` prints usage without an SDK
build. The captured Gradle build log remains in the report directory.

The `dyapp:android-test` image carries the source and a writable Gradle cache, so
the runner works in a throwaway container without network:

```bash
mkdir -p build/android-release && chmod 777 build/android-release
docker run --rm --network none --user 1000:1000 \
  -v "$PWD/build/android-release:/reports" \
  dyapp:android-test bash /app/tools/scripts/build-android-release.sh /reports
```

The image build caches the Release Gradle dependencies, Release lint included, by
running the Release tasks without the release Rust core and discarding their
packages. The runner uses offline mode and installs nothing. It requests
`assembleRelease` and `bundleRelease`, including release Rust JNI libraries; it
does not set `dyapp.prebuiltRust` and does not skip `lintVitalAnalyzeRelease`.

`build.log` retains Gradle output. On success `packages.json` records APK/AAB
bytes, SHA-256 and the required arm64-v8a/x86_64 entries. The verifier requires
nonempty manifest/dex/library members, rejects duplicate ZIP names and checks
64-bit little-endian ELF machine identifiers for the native libraries. This is
structural verification, not a signature, full executable or installation test.
The runner expects `app-release-unsigned.apk` and `app-release.aab` and copies
both verified packages into the report directory; signing and store distribution
remain planned. `release-sizes.yml` runs it on schedule and gates both packages
([size policy](ci-base.md#artifact-size-budgets)).

## Unsigned Apple Release packages

`build-apple-core.sh` uses the shared concise check frontend; its `--raw` first
argument preserves direct build diagnostics and the remaining scope/profile
arguments. Native release CI archives its raw frontend log alongside package
measurements and Xcode logs.

`tools/scripts/package-apple-release.py <ios|macos> <DYApp.app> <new-archive>`
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
architecture and filesystem layout.

`release-sizes.yml` requests native macOS-15 Release builds for device iOS arm64
and universal macOS arm64/x86_64. It calls `build-apple-core.sh` with `release`
(`ios-device` omits simulator libraries), generates the existing Xcode projects,
and runs `tools/scripts/apple-release-in-ci.py <ios|macos>` with
`CODE_SIGNING_ALLOWED=NO`. The producer builds configuration `Release`, packages
the app and records bytes/hash/source/compiler in `measurement.json`, together
with Xcode version and captured logs. The successful
[native measurement run](https://github.com/abbat/dyapp/actions/runs/39458581481)
refreshed the unsigned IPA and universal ZIP budgets on Xcode 16.4 (16F6).
The producer checks the real package bytes and writes `size.json`/`size.log`
using the exact `HEAD^..HEAD` source range. Keep toolchain/profile measurements
separate when changing the producer ([size policy](ci-base.md#artifact-size-budgets)).
The unsigned IPA and ZIP are kept as the run's `apple-ios-unsigned` and
`apple-macos-unsigned` artifacts. The project has no Apple account, so upstream
signing and notarization are not done: the [macOS](../manuals/macos.md) and
[iOS](../manuals/ios.md) manuals show how an account holder builds and signs a copy.

## What a working build would need (roadmap)

Not implemented. This is the order in which the gaps block each other:

1. Build `dyapp-ffi` instead of the four internal crates.
2. **iOS and macOS:** done in `tools/scripts/build-apple-core.sh` (Swift bindings and an
   XCFramework, see [FFI bindings → Apple](ffi-bindings.md#apple)); explicit Release
   core/package production is measured and size-gated in native CI.
   Signing, notarization and App Store distribution remain planned.
3. **Android:** done in `clients/android/app/build.gradle` (`cargo ndk` for arm64-v8a and
   x86_64, Kotlin bindings, JNA). The [offline Release runner](#unsigned-android-release)
   packages unsigned APK/AAB, measured and size-gated in scheduled CI. Release signing
   and store distribution remain planned.
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
