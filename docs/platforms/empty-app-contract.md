# Empty application acceptance contract

This is the implementation contract for the empty platform applications. It specifies required checks, not evidence that those checks have passed.

## Scope

Each application opens one real window or activity, renders `DYApp` and `Ready`, and terminates when closed. The ready element has the stable accessibility identifier/test tag/DOM id `app-ready`. Launch does not require networking, registration, Rust FFI, or any business feature. Initialization failures must fail the launch test; a placeholder bridge is not evidence of FFI integration.

Only the roots below are applications. Web is outside this milestone. Preserve SwiftUI for Apple and Compose for Android; do not add Tauri to SwiftUI.

## Required platform inventory

Commands below are the target interface to implement in the linked platform tasks. Missing commands, targets, projects, reports, or test cases are failures, never successful skips.

| Platform | Canonical root / framework | Minimum target / ABI | Build and artifact | Unit and UI checks | Execution |
| --- | --- | --- | --- | --- | --- |
| Linux | `linux/`, Tauri 1 with bundled static HTML | Debian 12+ / Ubuntu 24.04+, x86_64 | `cargo build --manifest-path linux/Cargo.toml --locked`; executable and packaged static frontend | Cargo unit tests; real executable launched under container Xvfb, assert visible window and rendered `app-ready`, close and assert exit | Non-root Linux Docker |
| Windows | `windows/`, Tauri 1 with bundled static HTML | Windows 10+, x86_64 | `cargo build --manifest-path windows/Cargo.toml --locked`; executable and packaged static frontend | Cargo unit tests; native UI automation launches built executable, asserts window and rendered `app-ready`, closes app | Native Windows CI |
| macOS | `macos/`, SwiftUI | macOS 11+, arm64 and x86_64 | `xcodebuild -project macos/DYApp.xcodeproj -scheme DYApp build`; `.app` | XCTest unit and XCUITest launch/render/close via `xcodebuild ... test`, result bundle | Native macOS CI |
| iOS | `ios/`, SwiftUI | iOS 14+, arm64 device and supported simulator ABI | `xcodebuild -project ios/DYApp.xcodeproj -scheme DYApp build`; simulator `.app`, unsigned device build | XCTest unit and XCUITest launch/render/terminate via `xcodebuild ... test -destination ...`, result bundle | Native macOS CI with iOS Simulator |
| Android | `android/`, Kotlin Compose | minSdk 26, compileSdk 37, targetSdk 36; arm64 device and x86_64 emulator | `./gradlew --offline --no-daemon :app:assembleDebug`; APK | `:app:testDebugUnitTest`, then `:app:connectedDebugAndroidTest` on software emulator; assert `app-ready` displayed and activity launch/termination | Non-root Linux Docker; emulator inside container |

Use a single coherent Tauri 1 configuration with `distDir`/bundled `devPath`, not a localhost development server. Android unit tests exercise application state and instrumentation exercises the canonical activity. Apple UI targets are separate from Swift Package unit targets. App state unit tests must verify a meaningful state transition or value; `assert(true)` and source-text checks do not count. A rendered browser page alone does not prove a Tauri executable launches.

## Runtime isolation

Every local suite, including Rust, workflow validation, runner regression tests, unit/integration/UI and coverage, runs in Docker as a nonzero UID, with all capabilities dropped and `no-new-privileges`. No privileged mode, host network/PID/IPC namespaces, Docker socket, host display/X11 socket, host service, or published port. Use container Xvfb and Android software emulation without mandatory KVM access. Source mounts are read-only; writable caches/reports use explicitly owned volumes or container directories. Image preparation is distinct from test execution: dependencies must already be cached before offline tests start.

Suites without peers use `network_mode: none`. Network suites use an internal Compose network with peers in neighboring containers and service-name discovery. Test assertions and readiness probes run inside containers; the host only orchestrates Docker and retrieves artifacts. A failure or unavailable peer must propagate a nonzero exit. Do not replace production network tests with a mock server and claim integration coverage.

## Local resource budget

Use `scripts/docker-local.py` through the Docker/UI runners for preparation and execution. A shared host lock serializes heavy runs. Image preparation builds services sequentially with Docker's legacy builder and an enforced CPU quota of `min(4, host CPUs)` (four on a workstation, two on a GitHub runner). Each successful image build prints only `OK <image>`; a failed build prints its full log and `FAILED <image> (exit N)`. Unsupported builder options fail rather than bypassing this quota. The legacy builder is deprecated; a replacement must retain the same enforced budget. Exception: in CI (`DYAPP_BUILD_CACHE=gha`, set by the `docker - android` job of `ci.yml` together with `setup-buildx-action` and `ghaction-github-runtime`) images are built with `docker buildx build --load` and a per-image GitHub Actions layer cache (`scope=<image with : replaced by ->`). BuildKit has no CPU quota, which is harmless on a 2-CPU runner. Dockerfiles `FROM` a local `dyapp:` image (`Dockerfile.network-test`) stay on the legacy builder, because the buildx container cannot see local images. The GitHub cache is capped at 10 GB per repository; older entries are evicted. `docker-local.py` exports the same budget as `DYAPP_CPUS` (and `DYAPP_EMULATOR_CPUS` = budget - 1) for the Compose `cpus:` limits, because Docker rejects a limit above the host CPU count. Cargo/native build workers follow `DYAPP_CPUS` in the dev image; Gradle uses two workers and Java sees at most four processors. Runtime CPU quotas sum to at most the budget within a suite: core/Linux the budget, Android emulator budget - 1 plus driver one, network peers/driver one each. Runners stop background services after a suite. Direct Compose invocations bypass the shared lock and are for explicit diagnostics only; do not run them concurrently with local suites.

## CI and evidence

The user approved on 2026-10-06: Linux/Android suites run locally in Docker; Apple/Windows UI suites run in native CI. Do not run native tests on the local host. CI includes the complete Docker inventory plus the three native suites. Pin the supported toolchain and dependencies; choose available Xcode versions without silently skipping destinations. The aggregate check includes all five platform jobs and fails for failed, skipped, or cancelled required suites. Native FFI consumers and SwiftLint/Detekt are separate stages after this milestone, as approved by the user.

Each suite records its command, artifact, test count, exit status and report (JUnit/CTest/Cargo output or `.xcresult`). Human-readable coverage is coverage.txt, with JSON/LCOV companions. CI exports reports from the directory identified by the coverage container run ID. Local UI reports are stored in `/reports/<container-hostname>` to keep runs separate. After a suite, retrieve its reports and tested executable/APK with `python3 scripts/export-ui-results.py linux /tmp/ai/linux-results` or `python3 scripts/export-ui-results.py android /tmp/ai/android-results`; the host only copies files. CI artifact upload requires a confirmed destination. Inventory must be nonempty. Missing tools, missing tests and unfinished platform jobs remain unverified. Final acceptance in `dyapp-awdk.4` requires actual successful runs, including UI, and deliberate failure injection proving runners propagate failures.
