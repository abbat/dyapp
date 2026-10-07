# CI Base: Rust Core Pipeline

> **Status:** derived from `.github/workflows/ci-base.yml`, `linting.yml`, `scripts/` and
> `docker/compose.*.yml` on 2026-10-06. Nothing here was run for this document.
> For the overall matrix, the other workflows and the required-checks question, see
> [ci-matrix.md](ci-matrix.md).

`ci-base.yml` (name **rust**) is `workflow_call` only. Its single caller is
`ci-matrix.yml` (job `core`), which `ci.yml` in turn calls. It has two jobs.

## Jobs

### `quality` → `linting.yml`

`linting.yml` (name **quality**) is `workflow_call` only, so it does not run by itself.
It has one job, **quality** (ubuntu-24.04, 60 min), with these steps:

1. `bash scripts/docker-test.sh prepare` builds the `dyapp:dev-prepared` image from
   `docker/Dockerfile.dev`. This is the only online step: dependencies, toolchain 1.99.0,
   cargo-llvm-cov 0.6.21, cargo-deny 0.19.9 and actionlint 1.7.12.
2. `bash scripts/docker-test.sh quality` runs `scripts/quality-in-container.sh` offline in a
   hardened container (no network, read-only, `cap_drop: ALL`, UID 999). That script runs, in order:

| Check | Command |
|-------|---------|
| Container isolation | `scripts/check-container-isolation.sh` |
| Repository rules | `scripts/check-repository.py` |
| Workflow lint | `actionlint` |
| Shell syntax | `bash -n scripts/*.sh` |
| Python tests | `python3 -m unittest discover -s tests` |
| Python lint | `flake8 scripts tools tests` |
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings` |
| Check | `cargo check --workspace --all-features --all-targets --locked --offline` |
| Tests | `cargo test --workspace --all-features --locked --offline -- --test-threads=1` |

### `core` → **build**

This job runs on ubuntu-24.04 with a 60-minute timeout. Each step is a `scripts/docker-test.sh`
target that runs offline in the prepared image:

| Step | Command | What it runs |
|------|---------|--------------|
| prepare | `docker-test.sh prepare` | as above |
| build | `docker-test.sh build` | `cargo build --release --workspace --locked --offline` |
| tests | `docker-test.sh test` | `cargo test --workspace --all-features --locked --offline -- --test-threads=1` |
| coverage | `docker-test.sh coverage` | `coverage-in-container.sh` + `check_coverage.py` (gate: [Coverage policy](../testing/README.md#coverage-policy)) |
| coverage export / coverage upload (`if: always()`) | `export-coverage.py`, `upload-artifact` | artifact **`rust-workspace-coverage`** (`coverage.json`, `.lcov`, `.txt`); `if-no-files-found: error` |
| deny | `docker-test.sh security` | `cargo deny check --disable-fetch advisories bans licenses` against advisory DBs baked into the image (it logs their commit and date) |
| network | `docker-test.sh network-prepare` + `network` | `docker/compose.network.yml`: two containers running `dyapp-bootstrap`'s `test-peer` binary (`bootstrap-a`, `bootstrap-b`) on an internal network; `network-test-in-container.py` checks `/health` and round-trips a signed profile (from `test-peer sign-profile`) through `/profiles` on each |

**Scope:** every Cargo step uses `--workspace`, which covers all 7 crates (`bootstrap`,
`ffi`, `identity`, `messaging`, `p2p-net`, `profile`, `video`). The platform apps under `linux/` and
`windows/` are separate manifests and are not part of this pipeline.

**Duplication:** `cargo test` runs twice, once inside `quality` and once in `core`, and coverage
runs the tests a third time under instrumentation.

**Freshness:** `security` uses the advisory DB snapshot taken when the image was built. It does
not fetch at run time. The online `cargo audit` lives in `security.yml`
([ci-matrix.md](ci-matrix.md#other-workflows)).

## What is not here

- **MSRV job:** none. The pinned toolchain equals the declared `rust-version`
  ([ci-matrix.md](ci-matrix.md#msrv)).
- **OS / toolchain matrix:** none. Rust core runs only on Linux with 1.99.0; there is no
  macOS/Windows or beta/nightly core build.
- **Release artifacts and codecov:** none. The only artifact is the coverage report.
- **Cross-compiling mobile targets** (`aarch64-apple-ios`, Android ABIs): none
  ([build.md](build.md)).

## Running locally

```bash
bash scripts/docker-test.sh prepare     # once, online
bash scripts/docker-test.sh quality     # = linting.yml
bash scripts/docker-test.sh build
bash scripts/docker-test.sh test
bash scripts/docker-test.sh coverage    # = make coverage
bash scripts/docker-test.sh security
bash scripts/docker-test.sh network-prepare && bash scripts/docker-test.sh network
bash scripts/docker-test.sh all         # all of the above + scripts/ui-test.sh all
```

## Related

- [ci-matrix.md](ci-matrix.md): entry point, platform jobs, other workflows, required checks.
- [Docker testing](../testing/docker.md).
- [Coverage policy](../testing/README.md#coverage-policy).
