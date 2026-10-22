# CI Base: Rust Core Pipeline

> **Status:** derived from `.github/workflows/ci-base.yml`, `tests/scripts/` and
> `tests/docker/compose.*.yml` on 2026-10-07. Nothing here was run for this document.
> For the overall matrix, the other workflows and the required-checks question, see
> [ci-matrix.md](ci-matrix.md).

`ci-base.yml` (name **rust**) is `workflow_call` only. Its single caller is `ci.yml` (job
`core`), so its checks show as `ci / core / quality`, `ci / core / build`, `ci / core / mutants` and
`ci / core / fuzz`. It has four jobs.

## Reproducible builds

Release check CLIs print one concise result and retain complete frontend logs.
Both `reproducible.yml` and `release-sizes.yml` select
`build/release-check-output` and include it in their evidence artifacts, along
with the existing JSON measurements and build logs. Add `--raw` to a release
checker for its direct diagnostic output; report-file consumers do not need it.

`tests/scripts/check-reproducible.py dyappd` (or `linux`) builds the release executable
twice with `--locked` in fresh independent Cargo target directories and compares
nonempty output SHA-256 hashes. It disables incremental builds and compiler
wrappers, remaps source and target paths, and fixes `SOURCE_DATE_EPOCH` to the
source commit time. The toolchain, source revision, lockfile hash, build logs and
executable hashes and byte counts are recorded in
`build/reproducible-<target>.json`. Native nightly reports provide measured
size evidence for that producer; they do not reuse the prepared-image budgets.

Run inside the prepared development image for `dyappd`, or the prepared Linux
image for `linux`, with this checkout mounted and a writable target directory.
An image without Git accepts `--revision <full-SHA> --source-date-epoch <seconds>`
together; obtain both values from the mounted checkout on the host. Linux release
builds also need a writable source copy: Tauri generates schema files beside its
manifest. Put that copy outside another Cargo workspace (for example `/build/`),
and ensure the image's Cargo registry sources are readable by the build user.
Each invocation retains both build trees and logs in `build/reproducible/`;
allow room for two complete release builds. On failure inspect the reported log
or compare the two executables; do not accept an absent output or reuse one
Cargo build directory for both runs. Compiler caches are intentionally bypassed.

This checks repeatability in the same toolchain, system libraries and environment;
it does not promise identical bytes across platforms or compiler versions.
`reproducible.yml` runs daily at 03:17 UTC and on manual dispatch, separately
from push/PR checks. It uses Rust 1.99.0 and ubuntu-24.04, skips Cargo build
caches, and uploads the reports and logs even on comparison failure. On
2026-10-15 two real release builds of each executable produced equal hashes
locally in the existing prepared images; this establishes C10 for the stated
same-environment scope. Nightly runner results remain independently observable.
Tests cover changed bytes, empty/missing files, a successful Cargo exit without
the second executable, and a failed build with host-supplied Git metadata.

## Artifact size budgets

`tools/scripts/check-artifact-size.py <name> <file> --base <ancestor> --report <json>`
compares file bytes with `tests/scripts/artifact-sizes.json`. Exactly 5% growth passes;
one byte beyond the boundary needs a nonempty `Size-Increase: <reason>` commit
trailer inside `base..HEAD`. Use the actual change range; an older trailer does
not authorize later growth. Missing/empty files, unknown artifact names, invalid
budgets, shallow history and unavailable/non-ancestor revisions fail closed.
Reports include measured bytes, baseline environment, growth and exception reasons.

The inventory separates prepared-image and native Ubuntu release `dyappd`/Linux
baselines. Native measurements were refreshed in the
[2026-10-20 release comparison run](https://github.com/abbat/dyapp/actions/runs/39409444833):
both pairs matched byte-for-byte. Its `dyappd` size gate failed against the old budget after
6.51% growth; the inventory now records the measured size. The prepared-image `dyappd` size
was measured by local `make build` on `13a45c7`. The prepared-image Linux budget remains at
its last measured value: the current test image's Cargo source cache is unreadable by its
non-root runtime user, so a comparable Release build could not be made there.
`reproducible.yml` checks the real first output of each verified pair against its
matching Ubuntu baseline, and archives `artifact-size-<target>.json`. Scheduled
and manual runs use the single source commit range `HEAD^..HEAD`, recorded as
full SHAs; an exception in an older commit cannot authorize new growth. Update
the reviewed budget for a lasting increase rather than relying on an old trailer.

Use the same environment and profile for each named artifact; a new producer
needs a separate measured baseline. The checker never edits budgets. Lower them
after reductions; increasing a budget weakens a check and is governed by
[A6](code-quality.md#trigger-list). Every delivery still runs the local A6 gate;
a hatch in a trigger path needs closed quorum proof, not only a reason.

`tools/scripts/report-artifact-size.py` collects bytes, SHA-256, source revision,
Rust compiler and platform without claiming that a budget passed. Missing/empty
files or unavailable provenance fail. `release-sizes.yml` schedules a native
Windows 2022 unsigned Release EXE producer at 04:17 UTC and on manual dispatch;
it archives `release-size-windows.json` and `artifact-size-windows.json`.
The Windows gate uses the measured unsigned EXE baseline from the successful
[native producer run](https://github.com/abbat/dyapp/actions/runs/39409459725),
with the exact `HEAD^..HEAD` source range. Push/debug smoke CI is separate.
Apple jobs in the same workflow request unsigned iOS device and universal macOS
Release packages with optimized Rust core libraries. They archive measurement
JSON, Xcode version, actual package size-gate reports and captured logs. Distinct
unsigned device IPA and universal macOS ZIP baselines come from the
[2026-10-20 native producer run](https://github.com/abbat/dyapp/actions/runs/39458581481) on
`9e6c832`: both packages built, then failed the old budgets after about 49% growth from the
MLS encryption core (openmls and its crypto provider), so the inventory records the measured
sizes ([producer details](build.md#unsigned-apple-release-packages)).

The Android job builds the `dyapp:android-test` image, runs the
[offline Release runner](build.md#unsigned-android-release) in it without network,
and gates the verified unsigned APK and AAB against baselines measured in the same
image. The APK and AAB baselines come from the same 2026-10-20 run on `9e6c832`, after the MLS core
grew them by about 17% and 19%, respectively. It archives the build log, `packages.json`,
`release-size-android-<kind>.json` and `artifact-size-android-<kind>.json`.

Auxiliary Python lint, dependency audit, advisory Rust analysis and Windows
release compilation also use the shared concise runner. Their full logs are
uploaded as `python-check-output`, `audit-check-output`,
`analysis-check-output` and in the existing release-size artifact. Advisory
`continue-on-error` policies are unchanged. SBOM, Clippy JSON/SARIF and the
unsafe-location listing remain data outputs for their report consumers.

## Jobs

All jobs run directly on the runner, without Docker. Every check is a step of
`tests/scripts/rust-check.sh`, the same script the dev image runs locally
([Running locally](#running-locally)). All jobs set `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CXXFLAGS=-g0` and `RUST_BACKTRACE=1`. Each job begins with
`tools/scripts/ci-setup.sh rust`, which:

- installs the apt packages that mirror `tools/docker/Dockerfile.dev`: build tools, clang,
  protobuf-compiler, libssl-dev, flake8, PyYAML and shellcheck;
- copies `actionlint` 1.7.12 and `gitleaks` 8.30.1 out of their official images, pinned to the
  digests `tools/docker/Dockerfile.dev` uses;
- downloads the `typos` 1.50.3, `taplo` 0.10.0, `cargo-shear` 1.14.0, SwiftLint 0.65.1,
  detekt 1.23.8 and zizmor 1.30.1 release files and checks them against the sha256 sums `tools/docker/Dockerfile.dev`
  uses (detekt runs on the runner's Java; the dev image installs OpenJDK 21);
- installs toolchain 1.99.0 with rustfmt, clippy and llvm-tools, and exports
  `RUSTUP_TOOLCHAIN`.

In the build job `taiki-e/install-action` then installs the pinned cargo-llvm-cov and
cargo-deny. `Swatinem/rust-cache` caches the dependency builds per job. Cargo steps run online
with `--locked`.

Every action in every workflow, GitHub's own included, is pinned to a commit SHA with the release
in a comment (`uses: owner/action@<sha> # v1.2.3`). Workflows without a job that writes default
the token to `contents: read`.

Dependabot (`.github/dependabot.yml`) opens version-update PRs weekly for GitHub Actions, Cargo
(the root workspace, `clients/linux`, `clients/windows`, `tests/fuzz`,
`tools/arch-lint`), Gradle (`clients/android`) and the Dockerfiles in
`tests/docker/` and `tools/docker/`. It waits 14 days after a release ([dependency age](code-quality.md#dependency-age)), groups minor and
patch updates into one PR per ecosystem and moves all `libp2p*` crates in one PR, majors included. Cargo ignores
libc 0.2.190 (`nix` fails to build for iOS against it), argon2 0.6+ (it needs blake2 0.11,
while snow in libp2p noise still pins 0.10), and aes-gcm 0.11+ (snow and webrtc still require
RustCrypto 0.10); the latter two introduce duplicates rejected by `cargo deny`. Review these
temporary ignores when the upstream dependencies move. Dependabot also applies these version
ignores to security-update PRs for the excluded versions. A Gradle PR needs its
[verification metadata](build.md#gradle-dependency-verification) regenerated. Vulnerability alerts and Dependabot
security updates are enabled in the repository settings; eligible security PRs skip the schedule
and the wait.

### `quality` → **quality**

This job runs on ubuntu-24.04 with a 60-minute timeout and a full-history checkout
(`fetch-depth: 0`). After setup and a `cargo fetch` of every tracked `Cargo.lock` it runs
`rust-check.sh quality`, which runs these checks in order:

| Check | Command |
|-------|---------|
| Small batches | `tests/scripts/check-batches.py`: [A4](code-quality.md#small-batches), at most 400 changed text lines per commit or a justified `Large-Change:` trailer; JSON counts and exception reasons uploaded as `commit-batches`, including on failure |
| Repository rules | `tests/scripts/check-repository.py`: whitespace, conflict markers, valid JSON/YAML, every workflow sets top-level `permissions:`, relative Markdown links and their anchors resolve, every file under `docs/` is reachable by links from `docs/README.md`, `AGENTS.md` at most 80 lines, [size budgets](code-quality.md#size-budgets), no tracker IDs, [commit messages](code-quality.md#commit-messages) |
| Dependency age | `tests/scripts/check-dependency-age.py`: no crates.io version younger than 14 days in a tracked `Cargo.lock` ([dependency age](code-quality.md#dependency-age)) |
| Trigger list | `tests/scripts/triggers.py`: every glob of the [trigger list](code-quality.md#trigger-list) matches a tracked file |
| Docs with the code | `tests/scripts/docs-sync.py`: commits after its baseline that change a config key, CLI flag, public API or Make target also change `docs/` ([rules](code-quality.md#docs-change-with-the-code)) |
| Workflow lint | `actionlint` |
| Workflow security | `zizmor --offline .github`: template injection, persisted checkout credentials, excessive permissions and the other offline audits; settings in `.github/zizmor.yml` |
| Secret scan | `gitleaks git --redact`: the whole git history; known false positives in `.gitleaksignore`, or `gitleaks:allow` on the line |
| Spelling | `typos`: the tracked source tree; allowed words in `_typos.toml` |
| TOML format | `taplo fmt --check`: every `*.toml`, style in `.taplo.toml` (4-space indent, 100 columns) |
| Shell lint | `shellcheck tests/scripts/*.sh tools/scripts/*.sh tools/build-scripts/*.sh` |
| Python tests | `python3 -m unittest discover -s tests` |
| Python lint | `flake8 scripts tools tests` |
| Swift lint | `swiftlint lint --strict --quiet --no-cache clients/ios clients/macos` |
| Kotlin lint | detekt CLI: `--config detekt.yml --input clients/android --build-upon-default-config` |
| Format | `cargo fmt --all -- --check` |
| Unused dependencies | `cargo shear`: every manifest's dependencies, dev and build ones included, against the imports; a false positive goes to `[package.metadata.cargo-shear] ignored` |
| Lint | `cargo clippy --workspace --all-features --all-targets --locked -- -D warnings` (it type-checks everything, tests included, so there is no separate `cargo check`); `missing_docs` is denied ([B9](code-quality.md#public-api-documentation)) |
| Explicit panic budgets | `tests/scripts/check-panics.py`: [B8](code-quality.md#explicit-panic-budgets), non-test workspace Clippy `unwrap_used`/`expect_used`/`panic` counts; JSON artifact `panic-budgets` when the checker ran |
| Rust API docs | `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --locked --no-deps` |

Quality/security commands use the [shared output contract](code-quality.md#check-output):
one `OK` per command or `FAIL`/`FIX`/`DETAILS`, with full raw command logs uploaded
as `quality-check-output` / `core-check-output` from `/tmp/ai/rust-check-output`.
JSON budget/artifact files remain unchanged. Database revision output from the
local offline security setup is kept in its raw log.

`quality` does not run `cargo test`: the tests run once, in `core`. Locally `make test` runs them.

### `core` → **build**

This job runs on ubuntu-24.04 with a 60-minute timeout and a full-history checkout
(`fetch-depth: 0`, for the changed-line coverage diff). Setup installs cargo-llvm-cov 0.9.1,
cargo-deny 0.20.2 and cargo-nextest 0.9.148. The instrumented target (without debuginfo) fits the
runner's free disk, so there is no `free disk` step: deleting the preinstalled SDKs took about 5
minutes.

The workspace is compiled **once**, with coverage instrumentation. The `cov-*` steps and
`network` re-derive the same environment from `cargo llvm-cov show-env`, with target dir
`build/llvm-cov-target`, which is also the directory rust-cache keeps. `cov-build` builds with
`cargo test --no-run`, which has the same unit graph as `cargo test`, so the test step rebuilds
nothing.

| Step | `rust-check.sh` step | What it runs |
|------|----------------------|--------------|
| build | `cov-build` | `cargo llvm-cov clean --workspace`; `cargo test --workspace --all-features --locked --no-run` |
| tests | `cov-test` | `cargo nextest run` and `cargo test --doc` on that build; [flaky tests](../testing/README.md#flaky-tests) |
| coverage | `cov-report` | `cargo llvm-cov report` → `build/coverage/coverage.{json,lcov,txt}` + `check_coverage.py` with `COVERAGE_BASE` = the PR base or `github.event.before` (gate: [Coverage policy](../testing/README.md#coverage-policy)) |
| coverage upload (`if: always()`) | — | `upload-artifact`: artifact **`rust-workspace-coverage`**; `if-no-files-found: error` |
| deny | `security` | `cargo deny --locked check advisories bans licenses sources`; it fetches the advisory DB at run time |
| network | `network` | reuses `dyapp-bootstrap`'s `dyappd`, also the client, from the build step (`cargo test` builds it for the bootstrap integration tests). `network-test.py --local` then starts three `dyappd` processes on TCP and QUIC `127.0.0.1:7071`–`:7073`, each with its own storage and its listen addresses as external (`DYAPPD_LISTEN`, `DYAPPD_EXTERNAL`, `DYAPPD_STORAGE_DIR`); the second and third use the first as seed, the third allows 5 requests per second. Through the client commands it checks `info`, publishes a signed profile over TCP and reads it back over QUIC, expects `STALE` on replay, checks that the stores are independent, the mailbox round trip and that a watching fetch gets the next envelope pushed; then that a DHT lookup through the third node finds all three, that every replica holder serves the profile, that an ack on one mailbox replica node clears the envelope on all of them, that an envelope put to one replica node reaches another after a watching fetch there, that a flood of 15 gets on the third node gets the burst answered and the rest `RATE_LIMITED`, and, after stopping the second node, that new replicas skip it. It also checks the deny list (a listed mailbox and IP group refused after SIGHUP, the first node's shared list received by the third), that restarted nodes keep their peer ID and that SIGHUP's `node state` line shows nothing pending once the nodes idle |

`core` does not run a release build; `make build` runs `tools/scripts/build-rust.sh`
(`cargo build --release`).

**Scope:** every Cargo step uses `--workspace`, which covers all 7 crates (`bootstrap`,
`ffi`, `identity`, `messaging`, `p2p-net`, `profile`, `video`). The platform apps under `clients/linux/` and
`clients/windows/` are separate manifests and are not part of this pipeline.

**Freshness:** in CI, `deny` fetches the current advisory DB. The local dev image instead uses
the snapshot taken when the image was built, offline with `--frozen`. `cargo audit` also
runs in `security.yml` ([ci-matrix.md](ci-matrix.md#other-workflows)).

### `mutants` → **mutants**

Practice B2 ([mutation testing](../testing/README.md#mutation-testing)). This job runs on
ubuntu-24.04 with a 120-minute timeout and a full-history checkout. Setup installs
cargo-mutants 27.1.0. Step `mutants` runs `rust-check.sh mutants` with `MUTANTS_BASE` = the PR
base or `github.event.before` (`origin/main` when that commit is absent): every mutant in the changed Rust code must be caught. Artifact
**`mutants-diff`** (`if: always()`) keeps the diff and the outcome lists. The weekly full run is
`mutants.yml` ([ci-matrix.md](ci-matrix.md#other-workflows)).

### `fuzz` → **fuzz**

Practice B4 ([fuzzing](../testing/README.md#fuzzing)). This job runs on ubuntu-24.04 with a
30-minute timeout. Setup installs cargo-fuzz 0.13.2; rust-cache keeps `build/fuzz`. Step `fuzz`
runs `rust-check.sh fuzz`: every target for 60 seconds, its saved corpus first. On failure the
artifact **`fuzz-crashes`** keeps the crash inputs and the raw check logs. The nightly hour per target is `fuzz.yml`
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

Locally the same `rust-check.sh` steps run offline in the hardened dev container: no network,
read-only, `cap_drop: ALL`, UID 1000. The entrypoint `rust-command-in-container.sh` checks the
isolation first.

```bash
bash tests/scripts/docker-test.sh prepare     # once, online
bash tests/scripts/docker-test.sh quality     # rust-check.sh quality
make build
bash tests/scripts/docker-test.sh test
bash tests/scripts/docker-test.sh coverage    # cov-build + cov-test + cov-report
make mutants                            # rust-check.sh mutants, against origin/main
make fuzz                               # rust-check.sh fuzz, 60 s per target
bash tests/scripts/docker-test.sh security
bash tests/scripts/docker-test.sh network-prepare && bash tests/scripts/docker-test.sh network  # three node containers
bash tests/scripts/docker-test.sh all         # all of the above + tests/scripts/ui-test.sh all
```

## Related

- [ci-matrix.md](ci-matrix.md): `ci.yml` entry point, platform jobs, other workflows, required checks.
- [Docker testing](../testing/docker.md).
- [Coverage policy](../testing/README.md#coverage-policy).
