# UI Testing: Practical Guide

> **Status:** derived from `tests/scripts/ui-test.sh`, `tests/docker/compose.ui.yml`, `tests/docker/Dockerfile.android-test`,
> `tests/docker/Dockerfile.linux-test`, `tests/scripts/*-test-*`, `clients/{ios,macos}/project.yml` and the test sources on
> 2026-10-06. Nothing here was run for this document; test counts are counted from source, not
> from logs. For the target contract see [empty-app-contract.md](../platforms/empty-app-contract.md);
> for the CI jobs see [ci-matrix.md](../development/ci-matrix.md).

Docker UI stages capture Gradle/emulator output in memory and print one `OK` line, or the
captured output and an `ERROR` line. `DYAPP_VERBOSE=1 make ui-test` streams live output. See
[local check output](../agents/project.md#local-check-output) for capture limits.

## What is tested

The UI suites test the **platform shells** (`clients/android/`, `clients/ios/`, `clients/macos/`, `clients/linux/`, `clients/windows/`).
The shells call the Rust core for a peer ID (see
[ci-matrix.md](../development/ci-matrix.md#what-the-platform-jobs-do-not-validate)); shared
iOS/macOS unit tests also check per-slot Keychain wrapping account isolation and network draft
file separation/corruption. They verify that erasing one UUID wrapping item leaves another slot
and the legacy account readable, and that a valid deleting tombstone remains visible when its
slot directory is missing while malformed or symlink markers are refused. Native tests also
check confirmed removal, unknown-file and damaged-slot refusal, survivor key/draft isolation,
legacy-account preservation, failed or incomplete Keychain erase with a visible deleting
candidate, confirmation recheck on retry, and retry after partial file cleanup. iOS and macOS UI
tests cancel removal, then confirm it, select the surviving Peer ID in the picker and reopen it
after restart. A shared native unit test also reopens the surviving slot after another slot is
removed and checks its active and draft network files plus stable Peer ID. Both Apple UI suites
also verify Russian removal labels and that the double-length pseudo-language confirmation
remains hittable. Shared native unit tests additionally check that an unreadable slot cannot
replace the active selection and that a lost selected Keychain item produces recovery without
creating another key. An isolated debug UI fixture corrupts only the wrapped key file after
creation: both Apple UI suites check the Russian unavailable label, hidden Remove action, and
recovery after restart with Retry but no implicit Create. A shared native unit test also checks
that UUID profile drafts stay separate and a peer-named root draft is copied once into its slot.
The migration test also removes the slot and checks that its slot draft is cleaned while the old
root copy remains unchanged.
Another shared unit test checks that empty birth parts stay absent, yearless February 29 is valid,
and an impossible full date is rejected through the Rust UniFFI assessment.
The Apple preview model test checks that every unset field stays unpublished, partial birth parts
are preserved, and an available year produces ordered inclusive age bounds.
The preview screen is reached from the editor without an attestation; native Apple UI checks
remain in CI.
The Apple draft adapter unit test imports old JSON without deleting it, confirms `client.db` takes
precedence on later loads, and checks that another identity keeps an independent empty draft.
The Apple publication view maps the Rust signed readback to a separate, immutable snapshot before
rendering the version and public fields. The iOS and macOS UI tests save a birth month, restart,
see a localized publish failure in Russian and the double-length pseudo-language with fresh
confirmations required, then publish to the local node and find the signed readback with only the
saved fields and version 1.
No suite
exercises messaging, matching or video. The tests check launch, render, app identity and close;
the Android, iOS and macOS UI tests and the desktop smoke also check that the peer ID is 64
lowercase hex characters. Desktop smoke chooses each storage mode and verifies the same Peer ID
after a second launch. Before Create it checks the first-run screen: the recommended available
store is preselected, a choice alone creates nothing, no identity action is shown and no element
with the `hidden` attribute is rendered; the ready screen repeats the last two checks. File-mode cases check the visible key-file path and backup guidance;
system-mode cases check that no file path is shown. The Android UI test also recreates the activity and
checks that the peer ID, unwrapped again from the stored identity key, stays the same; the
iOS and macOS `testRelaunchKeepsPeerId` relaunches the app for the same check.
The Android place picker test picks Germany, refuses to save unlisted text, picks Berlin from
the "Berl" suggestions, previews the country and place, then clears the country and checks both
are gone from the preview. The iOS `testPlacePickerSavesOnlyASuggestedPlace` searches for
Germany, refuses unlisted text, picks Berlin from the "Berl" suggestions and previews it.
The Android publish test checks that an unsaved edit clears and disables both confirmations
until saved, then confirms both, runs the proof without an active network and expects the
localized not-connected failure, retry label and cleared confirmations.
A second Android publish test checks and activates the emulator runner's loopback node
(`10.0.2.2:33445`), publishes a saved draft and expects the signed readback, then deletes the
`network` and `network-draft` files so the other tests still start without an active network.
Their network form UI tests save bootstrap rows and a network name, relaunch the app in the same
isolated slot root, and check that the values reappear in the automatic form while Ready remains
hidden. After a successful local-node Check and Continue, both clients relaunch with the same
Peer ID on Ready; the form is then available through Network connection. Shared unit tests reopen
two UUID slot folders and check that each retains its own draft and active settings.
Apple UI tests also open read-only Settings after creation, find the selected inventory row by
accessibility identifier and Peer ID, and verify the network editor link remains available.
They create a second identity, switch back to the first, and relaunch to confirm its Peer ID
persists. The later Settings matrix covers removal and error states. A DEBUG-only
launch argument seeds an invalid draft after explicit identity creation so the tests can check
the visible read error and disabled Save action; release builds ignore this argument.
They also check that a malformed bootstrap address receives an ordered row outcome after Check
and remains available on Retry. Rust's connection-check test covers interrupting a silent node;
the native cancel path belongs to the later Apple failure matrix.
The same native UI cases verify that Continue stays disabled after an invalid row; a shared
unit test verifies that calling Activate then leaves the active settings absent.
The shared Linux/Windows desktop smoke measures the Peer ID text at narrow and
wide layout widths: it must wrap when space is tight and return to one line
when space grows, without changing the identifier. Its final check also runs the short Peer ID
rule on fixed IDs: "@" and the last 6 characters, one more for each tail another known ID shares,
and a copy of the same ID in another slot counts as no collision. On first identity creation, desktop smoke
also opens Settings to verify the active identity's full Peer ID, its short form and key backend,
creates another identity there,
switches between both identities, cancels removal once, then confirms removal and relaunches to
verify the surviving identity and Settings inventory.
The English, Russian and pseudo-language runs check Settings actions, the active identity's
localized storage kind, and the failed network status in the selected catalogue.
It also checks that a profile with no selected fields previews an unknown age, no full-date warning,
and unchecked 18+ and public-data confirmations. It rejects publish without the public-data
acknowledgement and checks that editing clears both confirmations. A separate save/restart pair
checks a nonempty draft and its public preview. On Linux, another case uses a loopback `dyappd`
node to publish the reviewed fields with both confirmations, waits for the verified version,
then compares the node's signed profile payload with the locally saved draft and checks that the
post-publish UI readback fields, age range and full-date warning equal the reviewed preview. The
same Linux
fixture first attempts publication without an active network, checks that the draft survives,
and that search asks to connect with a retry, then connects and retries the publication. After it,
a search on the node shows the sample (the own profile or none) with the node's filters and no
failures; it must offer the place filter, and Germany with Berlin picked from the suggestions
finds nobody, since the published profile has no place. After activation, a node case turns on the app's own node in Settings: the form shows
its defaults, refuses a 3 MB disk limit, shows the running node ID, then stops it, and the script
checks the saved `node.json` is off. The desktop test then leaves the Network area for
Profile and comes back, checks that draft rows and failed row statuses survive navigation, and compares
the active network file byte for byte before and after a failed check. A deterministic
Rust unit test cancels and resumes a 30-bit puzzle; the UI smoke does not race the ordinary
11-bit puzzle.

| Platform | Where it runs | Unit | UI | Result check |
|----------|---------------|------|----|--------------|
| Android | Docker (local + CI) | 6 JUnit files (`app/src/test/.../`, including birth-part and network-result checks) | 22 instrumented (`AppUITests.kt` ×5, `ProfileDraftTests.kt` ×3, `ProfilePublishTests.kt` ×2, `AppLockUITests.kt` ×2, `LockSetupTests.kt` ×7, `PseudoLocaleTests.kt` ×2, `ActivityLifecycleTests.kt` ×1) | `check-junit-results.py`, `check-instrumentation-results.py` (≥3) |
| Linux | Docker (local + CI) | 16 cargo tests (shell, slots, key stores, network settings, bundled frontend) | 32 `desktop-smoke.py` cases under Xvfb and GNOME Keyring; 25 for the installed release client, which has no dyappd | `run-cargo-unit-binaries.py`, `desktop-smoke.py` |
| Windows | CI only (windows-2022) | 4 cargo tests (shell, credential backend, bundled frontend) | the same smoke cases except the seven that need dyappd (25) | same |
| iOS | CI only (macos-15) | Swift unit files in `clients/ios/Tests/`, including Keychain and network draft cases | XCUITest in `clients/ios/UITests/` | `check-apple-results.py` (≥3) |
| macOS | CI only (macos-15) | macOS unit files plus shared Keychain and network draft cases | XCUITest in `clients/macos/UITests/` | `check-apple-results.py` (≥3) |

**Empty or skipped is failure.** Every checker rejects a missing report, zero tests, any skipped
test and any failure: `check-junit-results.py` (JUnit XML), `run-cargo-unit-binaries.py` (empty
`--list` per binary), `check-instrumentation-results.py` (needs `OK (n tests)` with n ≥ 3 and
`INSTRUMENTATION_CODE: -1`, because `am instrument` exits 0 on failures), `check-apple-results.py`
(xcresult summary: ≥3 total, 0 failed, 0 skipped, result `Passed`). A test that passes only on a
retry is [flaky](README.md#flaky-tests): `xcodebuild test` retries a failed test up to twice
(`-retry-tests-on-failure -test-iterations 3`) and `check-flaky.py` reads the
`xcresulttool get test-results tests` tree; `am instrument` has no retries, so the Android
suite reruns up to twice and the tests listed by a failed run before a passing one are flaky
(`android instrumentation` when a failed run lists none). The desktop smoke runs 19 app
instances: system create/restart, then in that profile add a file identity, restart past a
pending slot left by a simulated crash, pick from the picker after `active-profile` is
corrupted, reopen it and remove it through the remove screen; add another file identity, show
recovery after its key is deleted and remove it from recovery (each removal must delete only
its slot folder and reopen the system identity); file create/restart, then
reopen the same key moved to the pre-slot root layout; a profile draft create/save/restart pair, which rejects an unlisted place, picks Berlin from the
suggestions, checks it after the restart and clears it with the country;
then Russian, pseudo-locale and an
injected rejection that hides the Profile heading. The restart in
each mode must report the same 64-character Peer ID. Successful runs print
`UI_SMOKE_PASS:<peer-id>` (empty for the recovery case) and exit 0; failures print
`UI_SMOKE_FAIL:` with the failed UI check or error before exiting 1.
The desktop profile unit test also saves an entirely unspecified draft, reloads it, and advances
the publish puzzle after an explicit 18+ attestation; gender and every other field remain optional.
The Debian client package smoke installs and unlocks GNOME Keyring inside a
fresh D-Bus/Xvfb session and distinct test user for each binary (installed DEB
and tarball). Both binaries run the full system and file storage checks. A
separate home is needed because GNOME Keyring persists the first binary's
`primary` identity across D-Bus sessions. Without a running Secret Service the
application correctly disables that choice, making the system-mode smoke fail
rather than silently falling back to file storage.

On Linux, `linux-test.sh` keeps Xvfb server stderr in the shared check log instead of
letting `xvfb-run` discard it. When the desktop smoke fails or times out, it adds up to
4 KiB of display/session state before `xvfb-run` cleans up: the local display number,
X11 socket presence, up to eight Xvfb PIDs with their process states, and whether Xauthority
and D-Bus session settings exist. It does not dump the environment, cookie contents or
session addresses. An absent process is evidence of absence at capture time, not a root
cause diagnosis. Startup failures before the smoke starts retain Xvfb stderr and their
nonzero exit status; a display failure without `UI_SMOKE_FAIL` still fails the negative
case. There are no added retries. Successful runs keep the usual single `OK` line.
The shared runner retains raw logs and bounds displayed diagnostics as described in
[local check output](../agents/project.md#local-check-output); native CI uploads them as
`linux-check-output`, while local Docker output identifies its retained log path.

### Localized strings

Practice D1. Every visible string of the shells lives in a catalogue, and the UI tests also run
on a pseudo-locale:

| Shell | Catalogue | Pseudo-locale test |
|-------|-----------|--------------------|
| Android | `res/values/strings.xml` and `res/values-ru/strings.xml`, read with `stringResource`; plurals go in `<plurals>` | `pseudoLocalesEnabled` in the debug build; `PseudoLocaleTests` renders the start screen with an en-XA context and expects its accented strings |
| iOS, macOS | `clients/ios/DYApp/Localizable.xcstrings` (String Catalog, shared by both projects); `Text("key")` looks the key up, plurals are catalogue variations | `testDoubleLengthPseudoLanguage` launches with `-NSDoubleLocalizedStrings YES` and expects the doubled, hittable ready text |
| Linux, Windows | `clients/desktop/ui/i18n/{en,ru}.json`; `app.js` fills each `data-i18n` element, plurals use `Intl.PluralRules` | `--pseudo-locale`: `app.js` accents every string and pads it by 40%, the smoke expects `[Réádý··]` |

The tests compare against the selected catalogue (Android, desktop) or expected language text (Apple).
`check-repository.py` fails on a Compose `Text("…")` literal under `clients/android/app/src/main`, on a
SwiftUI `Text("key")` whose key is not in the String Catalog, and on visible text in
`clients/desktop/ui/*.html` outside `<script>`/`<style>` or a `data-i18n` key missing from `en.json`.
Strings that are data, such as the peer ID, are not catalogued. English and Russian are
shipped for v0.0.1. Android and Apple use the OS app locale; desktop selects `ru.json` when
the primary browser locale is Russian and `en.json` otherwise. The repository quality check
requires every English catalogue key to have a nonempty Russian value. The Linux/Windows smoke,
Android instrumentation and Apple UI suites each exercise the Russian ready screen. The Linux
application menu entry also has a Russian description.

## Local Docker suites (Android, Linux)

Requires Docker with Compose. All commands run from the repository root.

```bash
bash tests/scripts/ui-test.sh prepare           # build images (online; also run by make prepare)
bash tests/scripts/ui-test.sh android           # = make ui-test-android
bash tests/scripts/ui-test.sh android-emulator  # = make ui-test-android-emulator
bash tests/scripts/ui-test.sh linux             # = make ui-test-linux
bash tests/scripts/ui-test.sh prototype         # = make ui-prototype; desktop prototype screenshots
bash tests/scripts/ui-test.sh all               # = make ui-test; the three above in order
bash tests/scripts/ui-test.sh shell [android|linux]   # = make ui-test-shell (android)
```

`all` ends with "Local Docker UI inventory passed; Apple/Windows native CI is required
separately" — it does not cover iOS, macOS or Windows. There are no `ui-test-all` or
`ui-test-linux-gui` targets and no X11-forwarded GUI mode.

Images build only at `prepare`; `run` uses `--pull never`, and test containers run with no network
(the emulator pair uses an internal network), read-only root, `cap_drop: ALL`, UID 1000, and
`check-container-isolation.sh` runs first.

### Android

- **Image** `dyapp:android-test` (`tests/docker/Dockerfile.android-test`): Debian bookworm, JDK 17,
  Android cmdline-tools (checksum-pinned), SDK platform 37.0, build-tools 36.0.0,
  system image `android-31;default;x86_64` (Android 12, the supported floor). It copies the real Gradle project `clients/android/` (its own
  `gradlew`; wrapper jar and Gradle 9.8.0 distribution checksum-verified; AGP 9.4.1 with built-in Kotlin and the Compose compiler plugin 2.4.20) and, as UID 1000, pre-builds `:app:assembleDebug`,
  `:app:assembleDebugAndroidTest` and `:app:compileDebugUnitTestKotlin`. No Rust `.so` or generated
  bindings are copied (there are none to copy).
  It also builds a host `build/debug/dyappd` and keeps only that binary.
- **`android`** → service `android-unit-test` → `android-test-in-container.sh unit`: offline
  `./gradlew --rerun-tasks :app:testDebugUnitTest`, then `check-junit-results.py`.
- **`android-emulator`** → service `android-emulator-test`, which `depends_on` service
  `android-emulator` being healthy:
  - `android-emulator` creates the AVD on the `android-avd` volume (the emulator wants ~7.4G free
    for its 6G minimum userdata partition, more than the RAM `/tmp` has) and starts the emulator headless with `-accel off` by default
    (software emulation: **no KVM needed**, but slow). With `DYAPP_KVM=1`, `ui-test.sh` adds `tests/docker/compose.kvm.yml`, which passes `/dev/kvm`, adds the container user to the group that owns it on the host (`DYAPP_KVM_GID`, taken from `stat -c %g /dev/kvm`) and sets `-accel on`; locally that is `DYAPP_KVM=1 make ui-test-android-emulator`. CI also opens `/dev/kvm` with a udev rule. Health check: `sys.boot_completed = 1` and
    the package manager responds, polled every 15 s for up to 120 retries.
    Before the emulator, it generates a low-cost test key under `/tmp` and starts that `dyappd` on
    `127.0.0.1:33445`, advertised as `10.0.2.2:33445`, the guest's address for the container's
    loopback; its logs go to `node/` on the reports volume.
  - `android-emulator-test` (`ui`) installs the prebuilt debug and androidTest APKs, runs
    `am instrument -w com.dyapp.test/androidx.test.runner.AndroidJUnitRunner`, validates the
    output, and pulls the app's `files/ui-ready.png` screenshot (must be non-empty). On failure it
    also captures `failure.png` and `window.txt`; `logcat.txt` is always captured.
  - The emulator container is started as a dependency; `docker-local.py` runs
    `docker compose stop` when the test run ends, so it is stopped but not removed.
  - Manual launcher check (e.g. the app icon): start the emulator alone with
    `DYAPP_KVM_GID=$(stat -c %g /dev/kvm) docker compose -p dyapp -f tests/docker/compose.ui.yml -f tests/docker/compose.kvm.yml --profile all up -d --wait android-emulator`.
    Copy `/app/build/android/app/outputs/apk/debug/app-debug.apk` out of `dyapp:android-test`
    (`docker create` + `docker cp`); the emulator container's root is read-only, so stream the
    APK in with `adb -s emulator-5554 exec-in 'cat > /data/local/tmp/app.apk' < app-debug.apk`
    (via `docker exec -i`) and `adb shell pm install`. On the 320x640 screen,
    `input keyevent KEYCODE_HOME` then `input swipe 160 480 160 60 500` opens the app drawer;
    `adb exec-out screencap -p > drawer.png` saves it. The system image has no icon-shape
    overlays, so only the default rounded-square mask can be seen.

### Linux

- **Image** `dyapp:linux-test` (`tests/docker/Dockerfile.linux-test`): Debian bookworm with GTK 3 /
  WebKit2GTK 4.1 dev libraries, Xvfb, D-Bus, GNOME Keyring and Rust 1.99.0 from a digest-pinned
  image. It builds the Tauri 2 app from `clients/linux/` (+ `clients/desktop/`) and writes the test-binary inventory
  `build/unit-build.json` at build time. The container has no checkout mount and no
  network, and `ui-test.sh` never builds images: after changing `clients/linux/`, `clients/desktop/` or the
  Dockerfile, run `make prepare` (or `bash tests/scripts/ui-test.sh prepare`) first, or
  `make ui-test-linux` tests the old binaries.
- **`linux`** → `linux-test.sh`, after the container isolation check. The script builds the app and
  the inventory itself when no inventory exists, which is how CI's **native - linux** runs it.
  Under `xvfb-run` + `dbus-run-session`, it unlocks a test GNOME Keyring, runs the
  unit binaries with `run-cargo-unit-binaries.py`, then `desktop-smoke.py` launches
  `build/debug/dyapp-linux` 34 times using isolated test profiles. The network cases check an
  empty list, a refused and a malformed seed row, and that the draft survives a restart unactivated.
  The loopback cases start `build/debug/dyappd` (built by the image, or by the script when
  missing), connect the form, activate the network, publish the profile and find it as the only search result. Then the script flips the
  signature byte of that record in the node's search index: the next search shows no person
  and lists the forged reply. This
  isolated fixture uses an 8-bit node-ID puzzle in both the node and test-mode client; production
  keeps 22 bits.
- **Screenshots.** `ui-test.sh linux` empties `build/ux-research/current/` (gitignored), mounts it at
  `/shots` and sets `DYAPP_UI_SHOTS`. At named checkpoints `ui-test.js` calls the test-only
  `ui_test_shot` command, which resizes the window to 440 and 1280 px wide (1400 px high), runs
  ImageMagick `import -window root` and restores the size; the Xvfb screen is 1280x1400. Files are
  `<state>-<lang>-<width>.png`; the first capture of a state wins. After the English pass,
  `linux-test.sh` runs `desktop-smoke.py` again with `DYAPP_SMOKE_RU=1` (every case gets
  `--ui-test-ru`) for the `-ru` files, so the screenshots roughly double the desktop smoke time.
  Without `DYAPP_UI_SHOTS` the command does nothing. A failed screenshot only logs to stderr.
  States: `setup`, `picker`, `ready`, `settings`, `remove`, `recovery`, `network-empty`,
  `network-failure`, `network-connected`, `profile-edit`, `profile-preview`,
  `profile-publish-failed`, `profile-published`, `search-not-connected`, `search-results`,
  `search-empty`, `search-error`. The in-progress network check and the publish puzzle finish
  too quickly to capture.

### Desktop prototype screenshots

`ui-test.sh prototype` (`make ui-prototype`) empties `build/ux-research/prototype/` and runs the
light `dyapp:prototype` image (Debian 12, WebKitGTK 4.1, Xvfb; no Rust build, also built by
`prepare`; the script and the prototype are mounted, so editing them needs no rebuild).
`prototype-shots.py` loads the static `clients/desktop/prototype/index.html` read-only
for each screen of [Desktop screens](../design/desktop-screens.md) in English and Russian, at 440
and 1280 px wide (800 px high), and saves the full page as `<screen>-<lang>-<width>.png`. It exits
non-zero when any page fails to load or save. The prototype has no logic, so this checks layout
and wording only; the maintainer reviews the files by eye.

### Reports on the host

Reports are written to `/reports/<container hostname>/` in the named volume `ui-reports`, not to a
host directory. `ui-test.sh` does not pass `--rm`, so the last containers remain and can be
exported:

```bash
python3 tests/scripts/export-ui-results.py android <dir>   # android-unit-test + android-emulator-test
python3 tests/scripts/export-ui-results.py linux   <dir>   # linux-test
```

The exporter prints `OK ui-export` or mapped failure details. `--raw` keeps
Docker diagnostics; exported reports and application files keep their formats.
`--help` describes the command without accessing Docker.

| Suite | Exported files |
|-------|----------------|
| `android-unit-test` | `gradle-version.txt`, `gradle-tasks.txt`, `test-results/` (JUnit XML), `hs_err*.log` on JVM crash |
| `android-emulator-test` | `instrumentation.txt`/`.json`, `instrumentation-<n>.txt` of failed runs, `ready.png`, `logcat.txt`, on failure `failure.png`, `window.txt`; plus `app-debug.apk` |
| `linux-test` | `desktop-ui-results.json`; plus the `dyapp-linux` binary |

In CI the Docker jobs upload **no** artifacts (logs only).

## Native CI suites (iOS, macOS, Windows)

These run only in `ci.yml`; see [ci-matrix.md](../development/ci-matrix.md#ciyml-jobs).

- **iOS / macOS:** XcodeGen 2.46.0 generates `<platform>/DYApp.xcodeproj` from
  `<platform>/project.yml` (targets `DYApp`, `DYAppTests`, `DYAppUITests`; scheme
  `DYApp`) after `tools/scripts/build-apple-core.sh <platform>` has built the XCFramework and Swift
  bindings the app target needs. There is no `.xcworkspace`, and the generated project is not committed.
  `apple-test-in-ci.sh` → `apple-test-in-ci.py` runs an unsigned `xcodebuild build`, then
  `xcodebuild test` into `$RUNNER_TEMP/apple-results/<platform>.xcresult` (iOS on the first
  available iPhone simulator, signed ad hoc because the simulator refuses an unsigned app the
  Keychain; macOS unsigned on `platform=macOS`), summarizes it with `xcresulttool` and
  applies `check-apple-results.py` and `check-flaky.py` (`tests.json`). Artifacts: `apple-ios`, `apple-macos`.
  The DEBUG-only `--no-device-passcode` launch argument makes the iOS missing-passcode
  lock test deterministic across simulators with different credential settings.
  The DEBUG-only `--ui-test-corrupt-network-draft` argument seeds a malformed draft in the
  selected UI-test slot after creation; it never runs in release builds.
  The DEBUG-only `--ui-test-fail-publish` argument makes profile publication fail as if the
  active network were unavailable, while the local node keeps running for later cases.
  Before XCTest, the runner builds a host `dyappd`, generates a low-cost test key and starts
  it on `127.0.0.1:33445` for the native network UI cases. Its node ID, log and build output
  stay in `$RUNNER_TEMP/apple-results/`; the runner waits at most 30 seconds for its loopback
  listener and terminates the process after tests. It never contacts a public bootstrap node.
  Shared native unit tests first discover the local node's Peer ID, then check a pinned address:
  the active file must be absent before Activate and contain that address afterward. iOS and
  macOS UI tests show Continue disabled before Check, enabled after the local node answers, and
  open the profile editor after explicit activation.
  The failure cases use `127.0.0.1:33446` as a closed loopback endpoint, alongside malformed,
  repeated and invalid-pin rows. They assert the row order and keep Continue disabled when no
  compatible node answered.
  A second local listener on `127.0.0.1:33447` accepts TCP without answering the peer handshake.
  Shared native tests cancel that pending check and retry it while asserting the previously
  activated network remains unchanged. iOS/macOS UI cases cancel, retry, and reopen the same
  saved draft after a process restart.
  Another shared unit case derives a valid wrong pin from the local node's Peer ID and checks
  both wrong-peer and wrong-network results with fresh sessions; both must reject Activate.
  iOS and macOS UI cases show the wrong-network row and disabled Continue for a named network
  against the public loopback node.
  `apple-test-in-ci.sh` exits 2 unless `GITHUB_ACTIONS=true`, so there is no supported local
  command. On a Mac you can run `tools/scripts/build-apple-core.sh`, generate the project with
  XcodeGen and run the scheme in Xcode yourself; that path is not scripted.
- **Windows:** `cargo build` + `cargo test --manifest-path clients/windows/Cargo.toml`, then
  `desktop-smoke.py` on the exe; artifact `windows-app-and-ui`. Locally on Windows:
  `cargo test --manifest-path clients/windows/Cargo.toml --locked` and
  `python tests/scripts/desktop-smoke.py build/debug/dyapp-windows.exe` (report goes to
  `%RUNNER_TEMP%` or `/reports/%HOSTNAME%`, so set `RUNNER_TEMP`).

## Not covered

- UI coverage (JaCoCo, Xcode coverage) is not collected; see
  [Coverage policy](README.md#ui--platform-coverage).
- The Android emulator runs with `-accel off`, without KVM, unless `DYAPP_KVM=1` is set; CI
  sets it.
- Product-level UI flows (onboarding, profiles, matching, messaging) have no tests yet.
- Desktop window and tray behaviour
  ([state table](../design/v0.0.1-client-screens.md#desktop-window-and-tray)) needs a real
  session: the Docker Xvfb has no tray host, so the Linux suite can check only the saved-state
  file and the “no tray” path; tray menus, display changes and the macOS Dock are checked by
  hand on each OS.

## Troubleshooting

Linux unit/smoke stages, the standalone desktop smoke CLI and Windows Cargo
checks use mapped concise output. Native Linux/Windows CI archives full logs
from `build/platform-check-output`; JSON unit manifests and desktop result
files keep their existing roles. Desktop failures include only the failed app
invocation output, so the expected `--break-ui` rejection is not mistaken for a
regression. Prepared Rust unit binaries retain their actual failure exit code
(including 101) and reject empty inventories. Add `--raw` to `linux-test.sh`,
`desktop-smoke.py` or `run-cargo-unit-binaries.py` for direct diagnostic mode.

Apple hosted tests and core builds also use mapped results, with raw logs in the
native artifacts. `apple-test-in-ci.sh` keeps its exit-2 rejection outside approved
CI; the Python frontend likewise refuses local execution. `--raw` selects direct
diagnostic output for these Apple entry points. Existing SDK-free command-plan
tests use raw mode; actual native builds/tests run only in the approved CI.
Android unit/UI work remains enclosed by Docker capture; the emulator itself is
a streaming service, not a finite quiet check. CI captures Compose service logs
and copies the shared reports volume through the retained unit-test container
using `export-ui-reports.py` (`--directory PATH` selects a different destination).
Unique directories under `build/platform-reports`
retain XML/JSON, screenshots and validator logs without deleting earlier exports.
Missing containers and empty copies fail. The `android-check-output` artifact
contains those reports and `build/platform-check-output` raw logs.

- **Emulator never healthy:** software emulation boot can take many minutes; the health check
  allows ~30 min. Look at the `android-emulator` container logs.
- **`OK (n tests)` with n < 3 rejected:** a test class was dropped or filtered; the checker requires
  the full instrumented inventory.
- **`Empty unit suite` (Linux):** a test binary in `build/unit-build.json` lists no tests; rebuild the
  image after changing tests (`ui-test.sh prepare`), since the inventory is created at image build.
- **Stale results after code changes:** sources are copied into the images at build time; rerun
  `prepare`.
