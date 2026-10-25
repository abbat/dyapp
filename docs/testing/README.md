# Testing Strategy & CI/CD

## Overview

Local Make checks retain complete Docker output and print `OK` on success or
`FAIL`/`FIX`/`DETAILS` with the raw-log path on failure. Use `DYAPP_VERBOSE=1` for live output. See
[local check output](../agents/project.md#local-check-output) for paths and retention.

Multi-layer testing. Rust line coverage is a ratchet: the workspace total never drops below a stored baseline and changed lines are ≥80% covered — see [Coverage policy](#coverage-policy).

## Test Layers

### 1. Unit Tests (In Each Crate)

Located in `#[cfg(test)]` modules within each crate.

**Identity** (rust/identity)
- SignedRecord: sign/verify, tampered payload, foreign key, short key
- Identity: secret key round trip keeps the Peer ID

**Profile** (rust/profile)
- Highest valid version wins; a tombstone supersedes and carries no fields but the proof of work
- Proof of work: pinned Argon2id output, `solve` round-trips and resumes
- Rejects a bad signature or invalid content

**P2P Network** (rust/p2p-net)
- A client dials a server over QUIC loopback and adds it to its Kademlia routing table

**Messaging** (rust/messaging)
- Message: lifecycle (Pending→Sent→Delivered→Read), encryption flags
- MLS: roundtrip, tampered, replayed and foreign messages, single-use Welcome, forged
  certificates, revoked devices, suite and lifetime checks, commits, exporter,
  restart from sealed state, wrong key and tampered seal
- HybridClock: wall-clock tracking, counter on a stalled or backward clock, observe, skew
  rejection, peer-ID tie-break

**Video** (rust/video)
- VideoSession: state machine (Idle→OfferCreated→Connected), SDP generation
- ICECandidate: type detection (host/srflx/relay), priority ranking
- CodecNegotiation: agreement algorithm, no agreement without a common codec

**Bootstrap** (rust/bootstrap)
- Mailbox: envelope once per id, size limits, fetch with challenge, ack, replay and forgery rejected, TTL
- Signed profile: version ordering (stale → error), tombstone kept and hidden from the list
- Replication: encode/decode, single+multiple shard failures, fault tolerance
- RateLimiter: per-peer limits, active peer tracking
- Media billing checks payload and framing charges in separate upload and reply windows, so
  test assertions do not depend on a wall-clock second boundary. Its per-peer quota assertions
  use a test-only fixed clock, so a loaded CI host cannot refill the quota between calls.

**FFI** (rust/ffi)
- VideoSession wrapper: async bridging, state management
- Type conversions: Rust → Swift/Kotlin/C

**Run:** `cargo test --workspace`

### 2. Integration Tests

Located in `tests/integration_tests.rs`.

Multi-peer scenario testing:
- **Video session**: offer/answer, ICE gathering, codec negotiation
- **Reputation**: peer tracking, trust scoring
- **Bootstrap**: replication fault tolerance; the node protocol end to end is in
  `rust/bootstrap/tests/protocol.rs` and the three-node
  `tests/scripts/network-test.py`. Its local Docker run checks that later ranged reads use the
  assembled media cache after the holder loses its shard metadata. See
  [bootstrap testing](../architecture/bootstrap.md#testing) and the
  [acceptance list](bootstrap-acceptance.md).
- **Rate limiting**: per-peer governors, active peer tracking
- **Hybrid logical clock**: causality preservation and the same order on every peer
- **Codec negotiation**: agreement algorithm with remote offers

**Run:** `cargo test --test integration_tests`

### 3. Property-Based Tests

Implemented with the existing bootstrap `proptest` dev-dependency, with 64 cases per
property:

- `config.rs`: nested assignment preserves the value and unrelated fields; file <
  environment < CLI precedence; string CLI values remain literal; section collisions and
  unknown options fail; media shard thresholds follow the documented range.
- `rust/bootstrap/tests/properties.rs`: SignedRecord protobuf round-trips preserve payload and
  signature bytes, signatures stay bound to their domain, and an altered payload fails
  verification; every wire and storage type round-trips (below).

Boundary cases (including zero, the threshold endpoints and integer extremes) are selected
explicitly rather than relying on random generation to discover them. Fixed-case unit tests
remain useful for specific regressions. These generated checks sample inputs; they are not
proofs for the entire input space. Round-trips do not establish compatibility with an older
encoder; historical wire fixtures serve that separate purpose. Fuzzing remains planned.

**Run:** `make test` includes all properties. For a focused run, use
`cargo test -p dyapp-bootstrap --locked prop_` (configuration) or `--test properties` (wire
and storage types) inside the prepared dev container.
Raise the configured case count for a larger experiment. Failures print the minimal input and seed;
regression seeds are saved under `/tmp/ai/dyapp-{config,wire,json}-proptest-regressions.txt`.
Preserve a failing seed or turn its minimized input into a fixed regression before discarding
a container; these temporary files are not committed automatically.

#### Round-trip properties

Every `message` and `enum` in `proto/*.proto` and every Rust type deriving both `Serialize` and
`Deserialize` has a property in `rust/bootstrap/tests/properties.rs`: protobuf messages satisfy
`decode(encode(x)) == x`, protobuf enums map back from their number and schema name, serde types
survive a JSON round trip (compared by `Debug`). Values come from `proptest_derive::Arbitrary`,
which the `proptest` feature of identity, profile, p2p-net, messaging and video derives; only the
bootstrap dev-dependencies enable it, so release builds do not contain it.
`tests/scripts/check-repository.py` (`make quality`) fails when a type has no property line
(`name: Type` inside a round-trip macro). The operator-written `rust/bootstrap/src/config.rs`
types are exempt; the configuration tests above cover them.

### 4. Compatibility Fixtures

`rust/bootstrap/tests/compatibility.rs` protects selected v1 contracts using committed
fixtures in `rust/bootstrap/tests/fixtures/` and the existing prost/serde_json dependencies:

- A signed profile with a fixed public test key: old protobuf bytes decode and verify;
  the current producer emits the same payload, signature and record bytes. Domain changes
  are detected even when current sign/verify round-trips still pass.
- A Message JSON baseline with Unicode, a newline, the maximum clock timestamp, status
  and optional byte arrays: legacy fields decode correctly and current output matches.
- Two selected operator diagnostics: an unknown config option and an invalid deny entry.
  The CLI test uses isolated temporary storage and clears inherited configuration/backtrace
  settings. Dynamic paths, timestamps and incidental messages are not snapshotted.
- Real-binary CLI tests check Russian and English operator diagnostics and deny-list confirmations,
  while the `status` field names and values remain stable across locales. Client argument tests
  also check translated errors and the current default port in address guidance. Config-loading
  tests cover localized option, seed, TURN, removed-key, startup validation and node-key guidance
  with stable English diagnostics and without replacing existing node data or a malformed network marker.
  Real-binary EN/RU checks cover missing config files and TOML syntax errors; the latter must
  retain file and position without echoing a secret on the invalid line. Type and TURN-secret-file
  errors are also checked in both locales; a type canary must stay out of stderr.

`make test` runs these checks. Review fixture diffs as changes to an external contract;
never auto-update a baseline just to make a failed check pass. For an intentional format
migration, add a versioned fixture and test the required legacy decoding behavior, explaining
the compatibility impact in the change. These fixtures capture a small baseline, not every
wire type or every old release. Generated properties complement them by covering more values.

No snapshot CLI is needed for these three fixtures. Consider a framework only if fixture
volume makes manual review and loading costly.

### 5. Fuzz Testing

See [Fuzzing](#fuzzing).

## CI/CD Pipeline

The workflows are described from the YAML in [CI matrix](../development/ci-matrix.md) (entry
point, platform jobs, other workflows) and [CI base](../development/ci-base.md) (Rust core
pipeline). In short:

- `ci.yml` runs:
  - the Rust core in Docker (quality, build, tests, coverage, `cargo deny`, two-peer network test);
  - Linux and Android shell apps in Docker;
  - iOS and macOS on macos-15;
  - Windows on windows-2022;
  - an aggregate check, **required**.
- The platform apps do not link the Rust core yet.
- codeql, security (cargo audit + SBOM) and python lint are separate workflows.
- `release.yml` builds and tests `dyappd` with `make deb image`; a `vX.Y.Z` tag also publishes it
  with build provenance ([ADR 0020](../decisions/0020-dyappd-release-channels.md)).

### Required Status Checks

The `required` summary checks out the same source and runs `check-ci-results.py`.
It requires the core, Docker, Linux, Apple and Windows job results, rejects empty
or missing inventories and treats skipped/cancelled results as failures. Its
concise output names only failed/missing jobs; the raw frontend log is archived
as `summary-check-output`.

**Not established.** Branch protection is not stored in the repository, and nobody has
confirmed which checks GitHub requires before merge. `required` is designed as the
single aggregate gate for `ci.yml`. A failing check fails its workflow run, but this
documentation does not promise that a PR is blocked. See
[ci-matrix.md](../development/ci-matrix.md#required-checks).

## Flaky tests

Practice B5. `tests/scripts/rust-check.sh` steps `test` and `cov-test` run the workspace tests with
cargo-nextest 0.9.148 (pinned in `tools/docker/Dockerfile.dev` and `ci-base.yml`), then the doctests
with `cargo test --doc`, which nextest does not run. `.config/nextest.toml` retries a failed test
twice and writes a JUnit report to `build/nextest/default/junit.xml` (`[store]`, so the coverage
build in `build/llvm-cov-target` writes it there too). A test that fails all
three attempts fails the step. XCUITest and Android UI tests retry the same way; see
[ui-testing.md](ui-testing.md#what-is-tested).

A test that passes only on a retry is flaky. `tests/scripts/check-flaky.py` reads the report and fails
the step when a flaky test is not in `[flaky]` of `tests/scripts/baselines.toml`, or when an entry
there is older than 7 days. The CI failure becomes a bead through the
[CI report import](../agents/beads.md#ci-failures). Quarantine the test with its JUnit
`classname name` (XCUITest: `suite name`; Android UI: `class method`) and the day it was found,
as a TOML date:

```toml
[flaky]
"dyapp-bootstrap::properties signed_record" = 2026-10-19
```

Within 7 days, fix the test and remove the entry; an expired entry fails `make test`, CI and the
Python suite in `make quality`, even when the test no longer flakes.
For traffic assertions, first cause a protocol request and wait for its response: the
`dyappd status` health/metrics scrape is not itself counted in `requests_total`.

## Mutation testing

Practice B2, with cargo-mutants 27.1.0 (pinned in `tools/docker/Dockerfile.dev`, `ci-base.yml` and
`mutants.yml`; settings in `.cargo/mutants.toml`). A mutant is a small change to the code, such as
`<` turned into `<=` or a function body replaced by a default value; a test should then fail.

**Changed code, every push.** `make mutants` (`rust-check.sh mutants`, CI job
[`mutants`](../development/ci-base.md#mutants--mutants)) diffs the Rust files against
`MUTANTS_BASE` (the push or PR base in CI; `origin/main` locally, for a new branch and after a
forced push) and runs
`cargo mutants --in-diff` on a copy of the tracked files. Every mutant in a changed region must be
caught: a missed one fails the step with its location and mutation, for example

```text
MISSED   rust/messaging/src/mls.rs:79:5: replace verify_member -> Result<String, Error> with Ok(String::new())
```

Locally, run it after committing and before pushing, since it diffs against `origin/main`. Each
mutant rebuilds the package and runs its tests (about a minute in `dyapp-bootstrap`). Changes to
test code alone produce no mutants.

**Exceptions.** Fix a missed mutant with a test. When a mutant cannot be caught, for example
because it makes the code hang, mark the item `#[mutants::skip]` (or narrower,
`#[mutants::exclude_re("...")]`) with a `//` reason on the same line or the line above; the crate
then needs a regular dependency on the `mutants` crate. `check-repository.py` rejects a mutants
attribute without a reason. `exclude_re` in `.cargo/mutants.toml` skips the `Session` methods that
need a live node: only `rust/bootstrap/tests/protocol` runs them, and cargo-mutants tests a
mutant with its own package's tests. The exclusion is reviewed by 2026-12-25
([#26](https://github.com/abbat/dyapp/issues/26)).

**Full run, weekly.** `.github/workflows/mutants.yml` (Sundays 05:23 UTC, or manually) tests all
mutants in 16 shards, two at a time each, and `tests/scripts/check-mutants.py` adds up their
`outcomes.json`. The score is caught and timed-out mutants over all viable ones; it must not drop
below `[mutants] score` in `tests/scripts/baselines.toml`, and the job asks to raise the value when the
score is 0.5 points above it. The first full run (2026-10-19) scored 72.2%: 614 of 2205 viable
mutants were missed, most of them in `node.rs`, `service.rs` and the client now in `bin/dyappd/client.rs`.

The check also fails when every mutant of a package is unviable. cargo-mutants builds each package
alone (`cargo test --package <name> --all-features`), so a crate's `proptest` feature must forward
the `proptest` feature of each workspace crate whose types it derives `Arbitrary` over. Until
2026-10-24 `dyapp-profile` did not, so its 76 mutants never ran.

**Security invariants.** The weekly check also fails on any missed mutant in the functions listed
under `[mutants.invariants]` in `tests/scripts/baselines.toml`: identity signing and verification,
profile validation, MLS membership and Welcome handling, media authorization and storage, and all
of `media_repair.rs`. There is no accepted-survivor list. Kill a new survivor with a test, or remove
the code that makes it equivalent. A scope entry that matches no mutant also fails the check, so a
rename cannot silently drop a function from the gate. This adds no CI time, because the full run
already tests these mutants. To check a change locally, run cargo-mutants with `-f <file>` and
`-F '<regex>'` from that table on a copy of the tree, as `rust-check.sh mutants` does.

## Fuzzing

Practice B4, with cargo-fuzz 0.13.2 and libfuzzer-sys (pinned in `tools/docker/Dockerfile.dev`,
`ci-base.yml` and `fuzz.yml`). `tests/fuzz/` is its own Cargo workspace with its own `Cargo.lock`; it
builds on the pinned stable toolchain because the targets run without a sanitizer (`-s none`):
safe Rust needs no AddressSanitizer, and panics, overflows (debug assertions are on), hangs and
memory blow-ups still fail.

Most targets feed one request handler of a bootstrap node, the code that parses what any peer
sends. `tests/fuzz/src/lib.rs` opens one node with the media and search roles in a temporary directory,
with no request, byte or ban limits, so inputs reach the parsers instead of the rate limiter. The
last four feed what a client reads from other people.

| Target | Handler | Input |
|--------|---------|-------|
| `node` | `Service::node` | `NodeRequest` (`/dyapp/node`) |
| `profile` | `Service::profile`, `dyapp_profile::verify` | `ProfileRequest` (`/dyapp/profile`) |
| `mailbox` | `Service::mailbox` | `MailboxRequest` (`/dyapp/mailbox`), envelopes included |
| `media` | `Service::media` | `MediaRequest` (`/dyapp/media`) |
| `search` | `Service::search` | `SearchRequest` (`/dyapp/search`) |
| `inventory` | `Service::search_inventory` | `InventoryRequest` (`/dyapp/inventory`), from a search peer |
| `digest` | `Service::search_digest` | `DigestRequest` (`/dyapp/digest`), from a search peer |
| `gossip` | `Service::search_gossip` | gossip bytes |
| `mls` | `mls::group_of`, `Device::create_group`, `Device::join`, `Group::process` | a KeyPackage, Welcome or group message, by the first byte |
| `sframe` | `sframe::Receiver::decrypt` | an SFrame video frame; the first byte is the metadata length |
| `sdp` | `RTCSessionDescription::answer`, `unmarshal_candidate` (rtc) | a call peer's SDP answer or ICE candidate |
| `preview` | `dyapp_ffi::check_preview` | a preview or profile thumbnail; seeds include small JPEGs and the malformed JPEG that exposed a `zune-jpeg` AVX2 panic |

The first input byte decides whether the signed records in the request are signed again by a fixed
key; its low bits pick the signature domain. A random signature never verifies, so without this the
payload parsers behind the signature check would stay out of reach. Fetch and ack requests still
stop at the nonce, which each input cannot know. A random MLS signature never verifies either,
so `mls` reaches the TLS decoders and signature checks but not the content behind them.
Responses that a node reads as a client, mailbox pushes have no target yet (planned).

`check-repository.py` requires a target for every `pub fn` of `Service` in
`rust/bootstrap/src/service.rs` and its `mailbox.rs`, `media.rs` and `search.rs` modules that takes a `request: …Request` or `data: &[u8]`: it looks for
`SERVICE.<name>(` in `tests/fuzz/fuzz_targets/`.

**Runs.** `make fuzz` (`rust-check.sh fuzz`) runs each target (`FUZZ_TARGETS`, default all) for
`FUZZ_SECONDS` (default 60): CI job [`fuzz`](../development/ci-base.md#fuzz--fuzz) on every push,
and `.github/workflows/fuzz.yml` for an hour per target every night. Both workflows cache the
`tests/fuzz` Cargo workspace against `build/fuzz`. The script passes `--fuzz-dir .` from
`tests/fuzz/` so cargo-fuzz opens that workspace. libFuzzer first runs every
input in `tests/fuzz/corpus/<target>/`, then mutates them; new inputs go to `build/fuzz-corpus/` and a
crash to `build/fuzz-artifacts/<target>/`, with the panic in the log.

**Corpus.** `tests/fuzz/corpus/` holds the inputs, minimized with libFuzzer's `-merge=1`; it is marked
binary in `.gitattributes`, so it has no diffs and no line count. To take in the inputs of a
nightly run (artifact `fuzz-<target>`), merge them into the target's directory with
`cargo fuzz run --fuzz-dir . -s none <target> corpus/<target> <new inputs> -- -merge=1`
from `tests/fuzz/` and commit.

**Crashes.** A crash fails the step; the CI failure becomes a bead through the
[CI report import](../agents/beads.md#ci-failures). Fix it, then copy the crash input
(`crash-<sha1>`) into `tests/fuzz/corpus/<target>/`: from then on every run replays it first, which makes
it the regression test.

## Coverage Policy

The search protocol suite includes deterministic worker saturation: it holds a
single-thread blocking pool, queues two admitted reads, and requires the next
read to receive `RATE_LIMITED` with a one-second retry while node info stays
responsive. Releasing the pool must complete the admitted reads. This complements
the ordinary concurrent-query test and keeps overload-path coverage independent
of how quickly an empty SQLite query finishes.

This is the single source of truth for coverage numbers; other docs link here.

| Item | Value |
|------|-------|
| Tool | cargo-llvm-cov 0.9.1 (pinned in `tools/docker/Dockerfile.dev` and `ci-base.yml`). `tests/scripts/rust-check.sh` steps `cov-build`, `cov-test` and `cov-report` make one instrumented build (`cargo llvm-cov show-env`), run the [tests](#flaky-tests) on it, then `cargo llvm-cov report`. Step `coverage` runs all three. |
| Metric | **Lines** (`data[0].totals.lines` of the LLVM JSON export). Regions, functions and branches are reported but not gated. |
| Aggregation | One number for the **whole workspace**. There is **no per-crate threshold**. |
| Threshold | Ratchet (practice B1): the total must not drop below `[coverage] lines` in `tests/scripts/baselines.toml` (88.4%, measured at 88.44% on 2026-10-23). When the total exceeds it by 0.5 points or more, the check prints the value to raise it to; raise it in the same change. Lowering it weakens the gate; it was lowered once, from 87.8%, when bootstrap's inline test modules moved to separate `tests.rs` and `tests/` files, which cargo-llvm-cov leaves out of the report, so their covered lines left the total. |
| Changed lines | At least **80%** (`MIN_DIFF`) of the instrumented Rust lines changed since `COVERAGE_BASE` must be covered: the working tree is diffed (`git diff -U0`) against it and matched with `coverage.lcov`. CI sets the PR base or the pushed-over commit; locally, for a new branch and after a forced push (the pushed-over commit is gone) it is `origin/main`. On failure the first 50 uncovered `path:line` are printed. |
| Scope check | Fails with "Partial workspace coverage" unless files from all 7 crates appear: bootstrap, ffi, identity, messaging, p2p-net, profile, video |
| Exclusions | `--ignore-filename-regex '/registry/\|/dyapp[.]uniffi[.]rs$'` — dependency sources and the generated UniFFI scaffolding. Hand-written `lib.rs` files are **not** excluded. |
| Outputs | `coverage.json`, `coverage.lcov`, `coverage.txt` |

### Where the gate runs

| Entry point | Gated? | Notes |
|-------------|-----------------|-------|
| GitHub Actions: `ci.yml` → `ci-base.yml`, step "coverage" | Yes | Runs `rust-check.sh cov-report` on the runner after `cov-build` and `cov-test`; reports uploaded as artifact `rust-workspace-coverage`. No codecov. |
| `make coverage` / `make coverage-check` | Yes | Same `docker-test.sh coverage` |
| `bash tests/scripts/docker-test.sh all`, `make test-all`, `make pre-push` | Yes | `all` includes coverage |
| `bash tests/scripts/check-quality.sh --coverage` | Yes | Delegates to `docker-test.sh coverage` |

Whether the CI check is **required for merge** depends on branch protection, which is not in the
repository and has not been verified.

### Coverage subprocesses

Rust integration tests that call `.env_clear()` on a child process must forward
`LLVM_PROFILE_FILE` from the test runner. Otherwise the instrumented child writes
`.profraw` files into its working directory and can break tests that assert that
directory stays empty. The fast pre-commit check catches this pattern in staged
Rust integration tests; CI still runs the instrumented test suite.

### Per-crate targets (not enforced)

These are aspirations for test planning. No script checks them.

| Crate | Target (lines) | Strategy |
|-------|----------------|----------|
| identity | 90% | Signing, verification, key handling |
| profile | 90% | Version ordering, tombstones, validation |
| p2p-net | 75% | Swarm setup, DHT routing |
| messaging | 90% | All message states, retry logic |
| video | 80% | Session machine, codec negotiation |
| bootstrap | 75% | Storage, replication, rate limit |
| ffi | 60% | Type conversions only (Rust layer tested) |

### UI / platform coverage

There is **no instrumented coverage** for Swift, Kotlin or web UI code: no `-enableCodeCoverage`,
JaCoCo/Kover or `test:coverage` script is wired into any gate. Any UI coverage percentage in
[ui-testing.md](ui-testing.md) is a target only.

### Saved report

None is kept in the repository. Reports go to `build/coverage[.<run id>]/`. A local Docker run
writes them into the `workspace-cache` volume, and `tests/scripts/export-coverage.py` copies them out.
When splitting inline Rust unit tests, keep their previously measured coverage:
`cargo-llvm-cov` excludes files named `tests.rs` or `*_tests.rs` by default.
Use ordinary module filenames for these moves; do not change the coverage filter
or lower the baseline to compensate for a source-file split.

The exporter prints `OK coverage-export` or mapped failure details; `--raw`
keeps direct Docker diagnostics. Exported coverage files retain their formats.

## Local Testing

### Before Committing

```bash
# Run all tests + lints
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings

# Check coverage locally (Docker; see Coverage policy)
make coverage
```

### Quick Test

```bash
# Fast subset
cargo test --lib messaging  # Just the messaging crate
cargo test --test integration_tests -- hybrid_clock
```

## Test Naming

All test functions follow pattern:

```
test_<module>_<behavior>_<outcome>
```

Examples:
- `test_hybrid_clock_ordering`
- `test_video_session_state_transitions`
- `test_bootstrap_cleanup_removes_expired`
- `test_hybrid_clock_observe_moves_past_remote`

## Debugging Tests

```bash
# Show println! output
RUST_LOG=debug cargo test -- --nocapture

# Run single test
cargo test test_hybrid_clock_ordering -- --exact

# Run with backtrace
RUST_BACKTRACE=1 cargo test
```

## Performance Testing (Future)

Benchmarks using `criterion`:

```rust
#[bench]
fn bench_hybrid_clock_now(b: &mut Bencher) {
    let clock = HybridClock::new();
    b.iter(|| clock.now());
}
```

**Run:** `cargo bench`

## Platform-Specific Tests

The platform apps are empty shells that do not link the Rust core. Their suites check launch,
ready-screen render, identity and close only. Details, test counts and report export:
[UI testing](ui-testing.md).

| Platform | Command | Runs where |
|----------|---------|------------|
| Android unit (1 JUnit) | `make ui-test-android` | Docker, local + CI |
| Android instrumented (5, API 31 emulator, no KVM) | `make ui-test-android-emulator` | Docker, local + CI |
| Linux (6 cargo tests + 7 Xvfb smoke cases) | `make ui-test-linux` | Docker, local + CI |
| All three Docker suites | `make ui-test` | Docker |
| iOS / macOS (1 XCTest + 5 XCUITest each) | none locally (`apple-test-in-ci.sh` exits 2 outside Actions) | CI, macos-15 |
| Windows (4 cargo tests + 7 smoke cases) | `cargo test --manifest-path clients/windows/Cargo.toml --locked` | CI, windows-2022 |

There are no GLib/ctest, .NET or FFI wrapper test suites.

## Deployment Checklist

Before shipping:

- [ ] All CI checks pass (test suite, fmt, clippy, coverage, audit)
- [ ] Integration tests pass locally
- [ ] Workspace line coverage at or above the baseline, changed lines ≥80% (`make coverage`)
- [ ] No clippy warnings
- [ ] Security audit clean
- [ ] Platform builds compile
- [ ] CHANGELOG.md updated
- [ ] Version bumped (Cargo.toml)

## Known Limitations

1. **No end-to-end multiparty tests** (3+ peers) — would require actual network
2. **No real WebRTC connection tests** — an offer is answered by a second peer connection in one process, but no test reaches `connected` or exchanges media (see [Video](../architecture/video.md#testing))
3. **No real codec tests** — codec info tested, actual transcoding skipped
4. **No performance benchmarks** — not required for Phase 1-9, added in Phase 10+

## Future Enhancements

- [ ] Fuzz testing (libfuzzer)
- [ ] Load testing (k6 scripts for bootstrap server)
- [ ] Multi-peer sync simulation (signed profiles, HLC-ordered messages)
- [ ] Hardware acceleration tests (iOS metal, Android Vulkan)
- [ ] Battery/power consumption profiling
