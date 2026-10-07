# CI Matrix and Workflow Inventory

> **Status:** derived from `.github/workflows/*.yml` on 2026-10-06. Nothing here was run for this
> document; "Expected" notes come from reading the YAML and scripts, not from CI logs.
> Branch protection lives in GitHub settings, not in the repository, so **which checks are
> required for merge is unknown** (see [Required checks](#required-checks)).

## Entry point

`ci.yml` (name **ci**) runs on push to `main`/`master`/`develop`, on every `pull_request` and on
`workflow_dispatch`. It has one job, `required` (name **matrix**), which calls the reusable `ci-matrix.yml`.

```
ci.yml  ──▶ ci-matrix.yml ("matrix")
              ├─ core     ──▶ ci-base.yml ──▶ quality ──▶ linting.yml
              │                           └─ core (Docker Rust inventory)
              ├─ docker   (matrix: linux, android)
              ├─ apple    (matrix: ios, macos)
              ├─ windows
              └─ summary  needs [core, docker, apple, windows], if: always()
```

`ci-matrix.yml` also has `workflow_dispatch`, so it can be started on its own.
`ci-base.yml` and `linting.yml` are `workflow_call` only. Rust-core jobs are documented in
[ci-base.md](ci-base.md).

## ci-matrix.yml jobs

| Job (check name) | Runner | What actually runs | Artifact | Status |
|------------------|--------|--------------------|----------|--------|
| `core` | — | Calls [ci-base.yml](ci-base.md) | `rust-workspace-coverage` | Working pipeline |
| `docker` → **docker - linux** | ubuntu-24.04 | `docker-local.py -f docker/compose.ui.yml --profile all build linux-test`; `ui-test.sh linux` → `linux-test-in-container.sh`: unit binaries from `linux/unit-build.json`, then `desktop-smoke.py` on the Tauri app under Xvfb | none (logs only) | Shell app |
| `docker` → **docker - android** | ubuntu-24.04 | build `android-unit-test`; `ui-test.sh android` (`:app:testDebugUnitTest`, JUnit check); `ui-test.sh android-emulator` (API 30 x86_64 emulator, KVM via `DYAPP_KVM=1` + `docker/compose.kvm.yml` after a `kvm` step opens `/dev/kvm`, installs debug + androidTest APKs, `am instrument`, `ready.png` screenshot) | none (logs only) | Shell app |
| `apple` → **native - ios** / **native - macos** | macos-15 | Downloads XcodeGen 2.44.1, generates `<platform>/DYApp.xcodeproj`, runs `apple-test-in-ci.sh` → `apple-test-in-ci.py`: unsigned `xcodebuild build` (iOS device generic; macOS universal arm64+x86_64), then XCTest/XCUITest (iOS on first available iPhone simulator) | `apple-ios`, `apple-macos` (`$RUNNER_TEMP/apple-results/`) | Shell app |
| `windows` → **native - windows** | windows-2022 | Rust 1.99.0; `cargo build` + `cargo test` of `windows/Cargo.toml`; `desktop-smoke.py` on the exe | `windows-app-and-ui` (exe + `desktop-ui-results.json`) | Shell app |
| `summary` → **required** | ubuntu-24.04 | Fails unless every `needs` result is `success` (failed, skipped or cancelled all fail) | — | Working |

### What the platform jobs do **not** validate

Every platform app is an empty shell. None of them links the Rust core:

- `android/.../RustBridge.kt`: `System.loadLibrary` is a commented-out TODO.
- `ios/DYApp/RustBridge.swift`: init/shutdown are TODOs; the XcodeGen specs have no Rust
  framework dependency.
- `linux/` and `windows/` are Tauri 1.8.3 apps with no dependency on workspace crates.

So a green matrix means "the shells build and their own tests pass". It does **not** mean an
APK/AAB, `.app` or installer containing the Rust core was built, signed or tested. No job
produces release artifacts, and there is no web job. See [build.md](build.md) and
[ffi-bindings.md](ffi-bindings.md) for the missing pieces.

## Other workflows

These are separate workflows, not part of `ci.yml`.

| Workflow | Triggers | Jobs | Notes |
|----------|----------|------|-------|
| `codeql.yml` (codeql) | push/PR to `master`, `develop`; weekly Sat 00:00 UTC | **analyze - cpp** (autobuild); **analyze - rust** | The repo has no C/C++ sources, so the `cpp` autobuild is expected to find nothing to analyze (unverified). In the Rust job every `cargo deny` step and pedantic clippy is `continue-on-error`; the "secrets" grep always succeeds; `clippy.sarif` is generated but **never uploaded**. Uses floating `stable`, not 1.99.0. |
| `security.yml` (security) | push/PR to `master`, `develop`; weekly Sun 00:00 UTC | **audit** (`rustsec/audit-check-action@v1`); **sbom** (`cargo install cargo-sbom`, artifact `sbom`) | Online, floating `stable`; separate from the offline `cargo deny` in ci-base. |
| `python-lint.yml` (python lint) | push/PR to `master`, `main`, `develop` touching `**.py` or `.flake8` | **flake8** (`flake8 .`) | Path-filtered: absent on PRs without Python changes. |

## Required checks

Repository files cannot show branch protection. The facts are:

- **Workflow failure:** any failing job above fails its workflow run.
- **Designed as the single gate:** `required` aggregates `core`, `docker`, `apple`
  and `windows`, so requiring only that check would cover the whole `ci.yml` matrix. It does
  **not** cover codeql, security or python lint.
- **Unverified:** whether `required` (or anything else) is configured as a required
  status check. Until someone confirms it in the repository settings, treat merge blocking as
  **not established**.

## MSRV

There is no separate MSRV job. All Rust builds in ci-base use the pinned toolchain
(`rust-toolchain.toml`: `1.99.0`), which equals `rust-version = "1.99"`, so the declared MSRV is
exactly the toolchain CI builds with. Nothing tests an older compiler, and `codeql.yml` and
`security.yml` use floating `stable`. Locally, `make msrv` runs `cargo check` in
the same pinned container. See [git-conventions.md](git-conventions.md#rust-version-msrv).

## Reproducing locally

| CI job | Local command |
|--------|---------------|
| ci-base `core` + `quality` | `bash scripts/docker-test.sh prepare` then `bash scripts/docker-test.sh all` (also runs UI suites) |
| docker - linux / android | `bash scripts/ui-test.sh prepare`; `bash scripts/ui-test.sh linux` / `android` / `android-emulator` |
| native - ios / macos | Not reproducible locally: `apple-test-in-ci.sh` exits 2 unless `GITHUB_ACTIONS=true` |
| native - windows | `cargo test --manifest-path windows/Cargo.toml --locked` on Windows |

## Related

- [ci-base.md](ci-base.md): the Rust core pipeline.
- [Coverage policy](../testing/README.md#coverage-policy).
- [Docker testing](../testing/docker.md).
