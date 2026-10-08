# CI Base: Rust Core Pipeline

> **Status:** derived from `.github/workflows/ci-base.yml`, `scripts/` and
> `docker/compose.*.yml` on 2026-10-07. Nothing here was run for this document.
> For the overall matrix, the other workflows and the required-checks question, see
> [ci-matrix.md](ci-matrix.md).

`ci-base.yml` (name **rust**) is `workflow_call` only. Its single caller is `ci.yml` (job
`core`), so its checks show as `ci / core / quality` and `ci / core / build`. It has two jobs.

## Jobs

Both jobs run directly on the runner, without Docker. Every check is a step of
`scripts/rust-check.sh`, the same script the dev image runs locally
([Running locally](#running-locally)). Both jobs set `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CXXFLAGS=-g0` and `RUST_BACKTRACE=1`. Each job begins with
`scripts/ci-setup.sh rust`, which:

- installs the apt packages that mirror `docker/Dockerfile.dev`: build tools, clang,
  protobuf-compiler, libssl-dev, flake8, PyYAML and shellcheck;
- copies `actionlint` 1.7.12 out of the official `rhysd/actionlint` image, pinned to the
  digest `docker/Dockerfile.dev` uses;
- installs toolchain 1.99.0 with rustfmt, clippy and llvm-tools, and exports
  `RUSTUP_TOOLCHAIN`.

In the build job `taiki-e/install-action` then installs the pinned cargo-llvm-cov and
cargo-deny. `Swatinem/rust-cache` caches the dependency builds per job. Cargo steps run online
with `--locked`.

### `quality` → **quality**

This job runs on ubuntu-24.04 with a 60-minute timeout. After setup it installs actionlint 1.7.12
and runs `rust-check.sh quality`, which runs these checks in order:

| Check | Command |
|-------|---------|
| Repository rules | `scripts/check-repository.py` |
| Workflow lint | `actionlint` |
| Shell syntax | `bash -n scripts/*.sh` |
| Python tests | `python3 -m unittest discover -s tests` |
| Python lint | `flake8 scripts tools tests` |
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings` (it type-checks everything, tests included, so there is no separate `cargo check`) |

`quality` does not run `cargo test`: the tests run once, in `core`. Locally `make test` runs them.

### `core` → **build**

This job runs on ubuntu-24.04 with a 60-minute timeout. Setup installs cargo-llvm-cov 0.9.1 and
cargo-deny 0.20.2. The instrumented target (without debuginfo) fits the runner's free disk, so
there is no `free disk` step: deleting the preinstalled SDKs took about 5 minutes.

The workspace is compiled **once**, with coverage instrumentation. The `cov-*` steps and
`network` re-derive the same environment from `cargo llvm-cov show-env`, with target dir
`target/llvm-cov-target`, which is also the directory rust-cache keeps. `cov-build` builds with
`cargo test --no-run`, which has the same unit graph as `cargo test`, so the test step rebuilds
nothing.

| Step | `rust-check.sh` step | What it runs |
|------|----------------------|--------------|
| build | `cov-build` | `cargo llvm-cov clean --workspace`; `cargo test --workspace --all-features --locked --no-run` |
| tests | `cov-test` | `cargo test --workspace --all-features --locked -- --test-threads=1` on that build |
| coverage | `cov-report` | `cargo llvm-cov report` → `target/coverage/coverage.{json,lcov,txt}` + `check_coverage.py` (gate: [Coverage policy](../testing/README.md#coverage-policy)) |
| coverage upload (`if: always()`) | — | `upload-artifact`: artifact **`rust-workspace-coverage`**; `if-no-files-found: error` |
| deny | `security` | `cargo deny --locked check advisories bans licenses`; it fetches the advisory DB at run time |
| network | `network` | reuses `dyapp-bootstrap`'s `dyapp-node` and `test-peer` from the build step (`cargo test` builds them for the bootstrap integration tests). `network-test.py --local` then starts three `dyapp-node` processes on TCP and QUIC `127.0.0.1:7071`–`:7073`, each with its own storage and its listen addresses as external (`DYAPP_NODE__LISTEN`, `DYAPP_NODE__EXTERNAL`, `DYAPP_NODE__STORAGE__DIR`); the second and third use the first as seed, the third allows 5 requests per second. Through `test-peer` it checks `info`, publishes a signed profile over TCP and reads it back over QUIC, expects `STALE` on replay, checks that the stores are independent and the mailbox round trip; then that a DHT lookup through the third node finds all three, that every replica holder serves the profile, that a flood of 15 gets on the third node gets the burst answered and the rest `RATE_LIMITED`, and, after stopping the second node, that new replicas skip it |

`core` does not run a release build; `rust-check.sh build` (`cargo build --release`) is the local
`make build`.

**Scope:** every Cargo step uses `--workspace`, which covers all 7 crates (`bootstrap`,
`ffi`, `identity`, `messaging`, `p2p-net`, `profile`, `video`). The platform apps under `linux/` and
`windows/` are separate manifests and are not part of this pipeline.

**Freshness:** in CI, `deny` fetches the current advisory DB. The local dev image instead uses
the snapshot taken when the image was built, offline with `--frozen`. `cargo audit` also
runs in `security.yml` ([ci-matrix.md](ci-matrix.md#other-workflows)).

## What is not here

- **MSRV job:** none. The pinned toolchain equals the declared `rust-version`
  ([ci-matrix.md](ci-matrix.md#msrv)).
- **OS / toolchain matrix:** none. Rust core runs only on Linux with 1.99.0; there is no
  macOS/Windows or beta/nightly core build.
- **Release artifacts and codecov:** none. The only artifact is the coverage report.
- **Cross-compiling mobile targets** (`aarch64-apple-ios`, Android ABIs): none
  ([build.md](build.md)).

## Running locally

Locally the same `rust-check.sh` steps run offline in the hardened dev container: no network,
read-only, `cap_drop: ALL`, UID 999. The entrypoint `rust-command-in-container.sh` checks the
isolation first.

```bash
bash scripts/docker-test.sh prepare     # once, online
bash scripts/docker-test.sh quality     # rust-check.sh quality
bash scripts/docker-test.sh build
bash scripts/docker-test.sh test
bash scripts/docker-test.sh coverage    # cov-build + cov-test + cov-report
bash scripts/docker-test.sh security
bash scripts/docker-test.sh network-prepare && bash scripts/docker-test.sh network  # three node containers
bash scripts/docker-test.sh all         # all of the above + scripts/ui-test.sh all
```

## Related

- [ci-matrix.md](ci-matrix.md): `ci.yml` entry point, platform jobs, other workflows, required checks.
- [Docker testing](../testing/docker.md).
- [Coverage policy](../testing/README.md#coverage-policy).
