# Project guide for AI agents

The root [AGENTS.md](../../AGENTS.md) is the short entry point that agents discover
automatically. This page holds the details. **The user's own instructions (global `CLAUDE.md`,
`~/.claude/instructions/`, direct requests) always win** over this page and over AGENTS.md.

## Layout

| Path | What it is |
|------|------------|
| `rust/` | Cargo workspace (`Cargo.toml` at the root): `bootstrap`, `ffi`, `identity`, `messaging`, `p2p-net`, `profile`, `video`. Toolchain pinned to 1.99.0 in `rust-toolchain.toml`. |
| `rust/bootstrap/src/bin/test-peer.rs` | The only binary in the workspace (used by network tests). |
| `rust/ffi` | UniFFI 0.32 crate (`cdylib` + `rlib`). Its bindings are **not** consumed by any app yet. |
| `android/` | Gradle app (Kotlin). Does not link the Rust core. |
| `ios/`, `macos/` | XcodeGen (`project.yml`) Swift apps. Do not link the Rust core. |
| `linux/`, `windows/` | Separate Tauri crates (excluded from the workspace) serving `desktop/ui`. |
| `proto/`, `tools/` | Protobuf schemas; arch-lint, codegen and helper tooling. |
| `docker/` | Test-image Dockerfiles and `compose.{dev,network,ui}.yml` (project name `dyapp`, build context is the repo root; `.dockerignore` stays at the root). |
| `scripts/` | Check scripts behind every Makefile target and CI job (`rust-check.sh`, `linux-test.sh`, `ci-setup.sh` for CI runners). |

Architecture: [overview](../architecture/overview.md), [FFI design](../architecture/ffi-design.md).

## Commands

Locally everything runs in Docker through `scripts/`; nothing needs a host Rust toolchain. CI runs
the same `scripts/rust-check.sh` and `scripts/linux-test.sh` directly on the runner after
`scripts/ci-setup.sh` (Android still uses Docker). Run
`make prepare` once to build the images (it needs network access and is the only target that
pulls anything).

| Goal | Command | Runs |
|------|---------|------|
| Unit tests | `make test` | `scripts/docker-test.sh test` |
| Integration | `make test-integration` | bootstrap `integration_tests` + network compose |
| Everything (pre-push) | `make test-all` | `scripts/docker-test.sh all` |
| fmt check | `make fmt` | `cargo fmt --all -- --check` |
| Lint / quality (pre-commit) | `make quality` (= `lint`, `check`) | `scripts/check-quality.sh` |
| Coverage gate (75% lines) | `make coverage` | `scripts/docker-test.sh coverage` |
| Security | `make security` (= `audit`, `deny`) | `scripts/docker-test.sh security` |
| MSRV / type check | `make msrv` | `cargo check --workspace --all-features --all-targets` |
| Rust API docs | `make doc` | output stays in the Docker volume, see [documentation.md](../development/documentation.md) |
| UI tests | `make ui-test`, `ui-test-android`, `ui-test-android-emulator`, `ui-test-linux` | see [ui-testing.md](../testing/ui-testing.md) |

There are no other targets (no `ui-test-all`, no npm). Details: [testing](../testing/README.md),
[Docker](../testing/docker.md), [CI](../development/ci-matrix.md).

## Limits to keep in mind

- The FFI layer is unfinished: no platform app calls Rust. Do not document or test as if it did.
- Docs must describe what the code does now; mark plans as plans.
- Code changes need an agreed scope; a docs task does not authorise code fixes.

## Rules for agents

- **No automatic installs** (packages, toolchains, SDKs, `sudo`) and **no deletion** of files or
  data without explicit confirmation from the user. The `rm -f` / `apt-get -y` forms in AGENTS.md
  only say *how* to avoid interactive prompts, not that these actions are allowed.
- Track work in GitHub Issues, or in beads when set up locally ([beads.md](beads.md)).
- Use lean-ctx when it is installed: [lean-ctx.md](lean-ctx.md).
- Commit and push only when the user has authorised it (for the session or in their
  instructions). End commits with the Co-Authored-By line for your model (see AGENTS.md).
- Never claim a check passed unless you ran it and saw it pass.
- Update `docs/` in the same commit as the code: a new feature, a behaviour fix, or a changed
  command, config or interface must leave the affected pages describing what the code actually
  does (planned parts marked as planned, outdated statements deleted, `docs/README.md` index and
  relative links/anchors updated). No automated link check exists yet.
