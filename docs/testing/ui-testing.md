# UI Testing: Practical Guide

> **Status:** derived from `scripts/ui-test.sh`, `docker/compose.ui.yml`, `docker/Dockerfile.android-test`,
> `docker/Dockerfile.linux-test`, `scripts/*-test-*`, `ios|macos/project.yml` and the test sources on
> 2026-10-06. Nothing here was run for this document; test counts are counted from source, not
> from logs. For the target contract see [empty-app-contract.md](../platforms/empty-app-contract.md);
> for the CI jobs see [ci-matrix.md](../development/ci-matrix.md).

## What is tested

The UI suites test the **empty platform shells** (`android/`, `ios/`, `macos/`, `linux/`,
`windows/`). None of them links the Rust core (see
[ci-matrix.md](../development/ci-matrix.md#what-the-platform-jobs-do-not-validate)), so no suite
exercises messaging, matching or video. The tests check launch, render of a ready screen, app
identity and close.

| Platform | Where it runs | Unit | UI | Result check |
|----------|---------------|------|----|--------------|
| Android | Docker (local + CI) | 1 JUnit (`app/src/test/.../AppTests.kt`) | 3 instrumented (`AppUITests.kt` ×2, `RustBridgeTests.kt` ×1 — `activityCanBeClosed`, not a Rust test) | `check-junit-results.py`, `check-instrumentation-results.py` (≥3) |
| Linux | Docker (local + CI) | 2 cargo tests (`desktop/main.rs`, `linux/tests/integration_tests.rs`) | 2 `desktop-smoke.py` cases under Xvfb | `run-cargo-unit-binaries.py`, `desktop-smoke.py` |
| Windows | CI only (windows-2022) | same 2 cargo tests (`windows/` includes `desktop/` and `linux/tests`) | same 2 smoke cases | same |
| iOS | CI only (macos-15) | 1 XCTest (`Tests/AppTests.swift`) | 2 XCUITest (`UITests/LaunchTests.swift`) | `check-apple-results.py` (≥3) |
| macOS | CI only (macos-15) | 1 XCTest | 2 XCUITest | `check-apple-results.py` (≥3) |

`ios/Tests/RustBridgeTests.swift` exists but is **not compiled**: `ios/project.yml` lists only
`Tests/AppTests.swift` for `DYAppTests`, and its three tests are placeholders anyway.

**Empty or skipped is failure.** Every checker rejects a missing report, zero tests, any skipped
test and any failure: `check-junit-results.py` (JUnit XML), `run-cargo-unit-binaries.py` (empty
`--list` per binary), `check-instrumentation-results.py` (needs `OK (n tests)` with n ≥ 3 and
`INSTRUMENTATION_CODE: -1`, because `am instrument` exits 0 on failures), `check-apple-results.py`
(xcresult summary: ≥3 total, 0 failed, 0 skipped, result `Passed`). The desktop smoke runs the app
twice: `--ui-test` must print `UI_SMOKE_PASS` and exit 0; `--ui-test --break-ui` must print
`UI_SMOKE_FAIL` and exit 1 (an injected hidden ready screen has to be detected).

## Local Docker suites (Android, Linux)

Requires Docker with Compose. All commands run from the repository root.

```bash
bash scripts/ui-test.sh prepare           # build images (online; also run by make prepare)
bash scripts/ui-test.sh android           # = make ui-test-android
bash scripts/ui-test.sh android-emulator  # = make ui-test-android-emulator
bash scripts/ui-test.sh linux             # = make ui-test-linux
bash scripts/ui-test.sh all               # = make ui-test; the three above in order
bash scripts/ui-test.sh shell [android|linux]   # = make ui-test-shell (android)
```

`all` ends with "Local Docker UI inventory passed; Apple/Windows native CI is required
separately" — it does not cover iOS, macOS or Windows. There are no `ui-test-all` or
`ui-test-linux-gui` targets and no X11-forwarded GUI mode.

Images build only at `prepare`; `run` uses `--pull never`, and test containers run with no network
(the emulator pair uses an internal network), read-only root, `cap_drop: ALL`, UID 999, and
`check-container-isolation.sh` runs first.

### Android

- **Image** `dyapp:android-test` (`docker/Dockerfile.android-test`): Debian bookworm, JDK 17,
  Gradle 8.2 and Android cmdline-tools (checksum-pinned), SDK platform 34, build-tools 34.0.0,
  system image `android-30;default;x86_64`. It copies the real Gradle project `android/` (its own
  `gradlew`, wrapper jar checksum-verified) and pre-builds `:app:assembleDebug`,
  `:app:assembleDebugAndroidTest` and `:app:compileDebugUnitTestKotlin`. No Rust `.so` or generated
  bindings are copied (there are none to copy).
- **`android`** → service `android-unit-test` → `android-test-in-container.sh unit`: offline
  `./gradlew --rerun-tasks :app:testDebugUnitTest`, then `check-junit-results.py`.
- **`android-emulator`** → service `android-emulator-test`, which `depends_on` service
  `android-emulator` being healthy:
  - `android-emulator` creates the AVD and starts the emulator headless with `-accel off`
    (software emulation: **no KVM needed**, but slow). Health check: `sys.boot_completed = 1` and
    the package manager responds, polled every 15 s for up to 120 retries.
  - `android-emulator-test` (`ui`) installs the prebuilt debug and androidTest APKs, runs
    `am instrument -w com.dyapp.test/androidx.test.runner.AndroidJUnitRunner`, validates the
    output, and pulls the app's `files/ui-ready.png` screenshot (must be non-empty). On failure it
    also captures `failure.png` and `window.txt`; `logcat.txt` is always captured.
  - The emulator container is started as a dependency; stop it afterwards with
    `docker compose -f docker/compose.ui.yml --profile all down` (after exporting reports).

### Linux

- **Image** `dyapp:linux-test` (`docker/Dockerfile.linux-test`): Debian bookworm with GTK 3 /
  WebKit2GTK 4.0 dev libraries, Xvfb, D-Bus and Rust 1.99.0 from a digest-pinned image. It builds
  the Tauri app from `linux/` (+ `desktop/`) and writes the test-binary inventory
  `linux/unit-build.json` at build time.
- **`linux`** → `linux-test-in-container.sh`: `run-cargo-unit-binaries.py` runs every test binary in
  the inventory, then `desktop-smoke.py` launches `linux/target/debug/dyapp-linux` under
  `xvfb-run` + `dbus-run-session`.

### Reports on the host

Reports are written to `/reports/<container hostname>/` in the named volume `ui-reports`, not to a
host directory. `ui-test.sh` does not pass `--rm`, so the last containers remain and can be
exported:

```bash
python3 scripts/export-ui-results.py android <dir>   # android-unit-test + android-emulator-test
python3 scripts/export-ui-results.py linux   <dir>   # linux-test
```

| Suite | Exported files |
|-------|----------------|
| `android-unit-test` | `gradle-version.txt`, `gradle-tasks.txt`, `test-results/` (JUnit XML), `hs_err*.log` on JVM crash |
| `android-emulator-test` | `instrumentation.txt`/`.json`, `ready.png`, `logcat.txt`, on failure `failure.png`, `window.txt`; plus `app-debug.apk` |
| `linux-test` | `desktop-ui-results.json`; plus the `dyapp-linux` binary |

In CI the Docker jobs upload **no** artifacts (logs only).

## Native CI suites (iOS, macOS, Windows)

These run only in `ci-matrix.yml`; see [ci-matrix.md](../development/ci-matrix.md#ci-matrixyml-jobs).

- **iOS / macOS:** XcodeGen 2.44.1 generates `<platform>/DYApp.xcodeproj` from
  `<platform>/project.yml` (targets `DYApp`, `DYAppTests`, `DYAppUITests`; scheme
  `DYApp`). There is no `.xcworkspace`, and the generated project is not committed.
  `apple-test-in-ci.sh` → `apple-test-in-ci.py` runs an unsigned `xcodebuild build`, then
  `xcodebuild test` into `$RUNNER_TEMP/apple-results/<platform>.xcresult` (iOS on the first
  available iPhone simulator, macOS on `platform=macOS`), summarizes it with `xcresulttool` and
  applies `check-apple-results.py`. Artifacts: `apple-ios`, `apple-macos`.
  `apple-test-in-ci.sh` exits 2 unless `GITHUB_ACTIONS=true`, so there is no supported local
  command. On a Mac you can generate the project with XcodeGen and run the scheme in Xcode
  yourself; that path is not scripted. `ios/Package.swift` and `macos/Package.swift` exist but
  are not used by CI.
- **Windows:** `cargo build` + `cargo test --manifest-path windows/Cargo.toml`, then
  `desktop-smoke.py` on the exe; artifact `windows-app-and-ui`. Locally on Windows:
  `cargo test --manifest-path windows/Cargo.toml --locked` and
  `python scripts/desktop-smoke.py windows/target/debug/dyapp-windows.exe` (report goes to
  `%RUNNER_TEMP%` or `/reports/%HOSTNAME%`, so set `RUNNER_TEMP`).

## Not covered

- UI coverage (JaCoCo, Xcode coverage) is not collected; see
  [Coverage policy](README.md#ui--platform-coverage).
- The Android emulator runs with `-accel off`, without KVM.
- Product-level UI flows (onboarding, profiles, matching, messaging) have no tests yet.

## Troubleshooting

- **Emulator never healthy:** software emulation boot can take many minutes; the health check
  allows ~30 min. Look at the `android-emulator` container logs.
- **`OK (n tests)` with n < 3 rejected:** a test class was dropped or filtered; the checker requires
  the full instrumented inventory.
- **`Empty unit suite` (Linux):** a test binary in `unit-build.json` lists no tests; rebuild the
  image after changing tests (`ui-test.sh prepare`), since the inventory is created at image build.
- **Stale results after code changes:** sources are copied into the images at build time; rerun
  `prepare`.
