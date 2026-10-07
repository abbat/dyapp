# Testing Strategy & CI/CD

## Overview

Multi-layer testing. Rust coverage is gated at **75% line coverage on the workspace total** — see [Coverage policy](#coverage-policy).

## Test Layers

### 1. Unit Tests (In Each Crate)

Located in `#[cfg(test)]` modules within each crate.

**Identity** (rust/identity)
- SignedRecord: sign/verify, tampered payload, foreign key, short key
- Identity: secret key round trip keeps the Peer ID

**Profile** (rust/profile)
- Highest valid version wins; a tombstone supersedes and must carry no fields
- Rejects a bad signature or invalid content

**P2P Network** (rust/p2p-net)
- A client dials a server over QUIC loopback and adds it to its Kademlia routing table

**Messaging** (rust/messaging)
- Message: lifecycle (Pending→Sent→Delivered→Read), encryption flags
- MessageQueue: enqueue/dequeue, pending filtering, mark/remove delivered (no TTL; `retry_failed` only in integration tests, first retry)
- E2EEncryption: algorithm config, roundtrip encryption/decryption, signatures
- LamportClock: increment, observe, merge, clock synchronization

**Video** (rust/video)
- VideoSession: state machine (Idle→OfferCreated→Connected), SDP generation
- ICECandidate: type detection (host/srflx/relay), priority ranking
- CodecNegotiation: agreement algorithm, fallback behavior
- FrameEncryption: frame counter, roundtrip encrypt/decrypt

**Bootstrap** (rust/bootstrap)
- Mailbox: envelope once per id, size limits, fetch with challenge, ack, replay and forgery rejected, TTL
- Signed profile: version ordering (stale → error), tombstone kept and hidden from the list
- Replication: encode/decode, single+multiple shard failures, fault tolerance
- RateLimiter: per-peer limits, active peer tracking

**FFI** (rust/ffi)
- VideoSession wrapper: async bridging, state management
- Type conversions: Rust → Swift/Kotlin/C

**Run:** `cargo test --workspace`

### 2. Integration Tests

Located in `tests/integration_tests.rs`.

Multi-peer scenario testing:
- **Message relay**: offline queue, delivery, retry with exponential backoff
- **Signed profile over HTTP**: protobuf POST/GET/list, stale version (409), forged key and garbage (400), tombstone
- **Video session**: offer/answer, ICE gathering, codec negotiation
- **Reputation**: peer tracking, trust scoring
- **Bootstrap**: message storage, replication fault tolerance
- **Rate limiting**: per-peer governors, active peer tracking
- **Lamport clock**: causality preservation across peers
- **Codec negotiation**: agreement algorithm with remote offers

**Run:** `cargo test --test integration_tests`

### 3. Property-Based Tests

Using `proptest` crate in each module.

**Examples:**
```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_lamport_clock_monotonic(a in 1u64..100, b in 1u64..100) {
            let clock = LamportClock::new();
            clock.observe(a);
            clock.observe(b);
            assert!(clock.current() > a && clock.current() > b);
        }
    }
}
```

**Run:** `cargo test proptest`

### 4. Fuzz Testing (Future)

```bash
cargo +nightly fuzz run fuzz_message_parsing
```

Will fuzzer-test:
- Protobuf deserialization
- SDP parsing
- ICE candidate parsing

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

### Required Status Checks

**Not established.** Branch protection is not stored in the repository, and nobody has
confirmed which checks GitHub requires before merge. `required` is designed as the
single aggregate gate for `ci.yml`. A failing check fails its workflow run, but this
documentation does not promise that a PR is blocked. See
[ci-matrix.md](../development/ci-matrix.md#required-checks).

## Coverage Policy

This is the single source of truth for coverage numbers; other docs link here.

| Item | Value |
|------|-------|
| Tool | cargo-llvm-cov 0.9.1 (pinned in `docker/Dockerfile.dev` and `ci-base.yml`). `scripts/rust-check.sh` steps `cov-build`, `cov-test` and `cov-report` make one instrumented build (`cargo llvm-cov show-env`), run `cargo test --workspace --all-features --locked` on it, then `cargo llvm-cov report`. Step `coverage` runs all three. |
| Metric | **Lines** (`data[0].totals.lines` of the LLVM JSON export). Regions, functions and branches are reported but not gated. |
| Aggregation | One number for the **whole workspace**. There is **no per-crate threshold**. |
| Threshold | **75%** (`MIN_COVERAGE` in `scripts/check_coverage.py`) |
| Scope check | Fails with "Partial workspace coverage" unless files from all 7 crates appear: bootstrap, ffi, identity, messaging, p2p-net, profile, video |
| Exclusions | `--ignore-filename-regex '/registry/\|/dyapp[.]uniffi[.]rs$'` — dependency sources and the generated UniFFI scaffolding. Hand-written `lib.rs` files are **not** excluded. |
| Outputs | `coverage.json`, `coverage.lcov`, `coverage.txt` |

### Where the gate runs

| Entry point | Fails on < 75%? | Notes |
|-------------|-----------------|-------|
| GitHub Actions: `ci.yml` → `ci-base.yml`, step "coverage" | Yes | Runs `rust-check.sh cov-report` on the runner after `cov-build` and `cov-test`; reports uploaded as artifact `rust-workspace-coverage`. No codecov. |
| `make coverage` / `make coverage-check` | Yes | Same `docker-test.sh coverage` |
| `bash scripts/docker-test.sh all`, `make test-all`, `make pre-push` | Yes | `all` includes coverage |
| pre-commit `pre-push` stage | Yes | Runs `docker-test.sh all`; skipped by `git push --no-verify` or if hooks are not installed |
| `bash scripts/check-quality.sh --coverage` | Yes | Delegates to `docker-test.sh coverage` |

Whether the CI check is **required for merge** depends on branch protection, which is not in the
repository and has not been verified.

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

None is kept in the repository. Reports go to `target/coverage[.<run id>]/`. A local Docker run
writes them into the `workspace-cache` volume, and `scripts/export-coverage.py` copies them out.

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
cargo test --test integration_tests -- message_offline
```

## Test Naming

All test functions follow pattern:

```
test_<module>_<behavior>_<outcome>
```

Examples:
- `test_message_queue_enqueue_adds_entry`
- `test_video_session_state_transitions`
- `test_bootstrap_cleanup_removes_expired`
- `test_lamport_clock_observe_advances_properly`

## Debugging Tests

```bash
# Show println! output
RUST_LOG=debug cargo test -- --nocapture

# Run single test
cargo test test_message_queue_enqueue -- --exact

# Run with backtrace
RUST_BACKTRACE=1 cargo test
```

## Performance Testing (Future)

Benchmarks using `criterion`:

```rust
#[bench]
fn bench_message_queue_dequeue(b: &mut Bencher) {
    let mut queue = MessageQueue::new();
    queue.enqueue(msg);
    b.iter(|| queue.dequeue());
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
| Android instrumented (3, API 31 emulator, no KVM) | `make ui-test-android-emulator` | Docker, local + CI |
| Linux (2 cargo tests + 2 Xvfb smoke cases) | `make ui-test-linux` | Docker, local + CI |
| All three Docker suites | `make ui-test` | Docker |
| iOS / macOS (1 XCTest + 2 XCUITest each) | none locally (`apple-test-in-ci.sh` exits 2 outside Actions) | CI, macos-15 |
| Windows (2 cargo tests + 2 smoke cases) | `cargo test --manifest-path windows/Cargo.toml --locked` | CI, windows-2022 |

There are no GLib/ctest, .NET or FFI wrapper test suites.

## Deployment Checklist

Before shipping:

- [ ] All CI checks pass (test suite, fmt, clippy, coverage, audit)
- [ ] Integration tests pass locally
- [ ] Workspace line coverage ≥75% (`make coverage`)
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
