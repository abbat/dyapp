# Code Quality

What is checked, by which command, and what is only a convention. Per-language linter details are
in [linting.md](linting.md).

## Local checks

All checks run in Docker; build the image once with `make prepare`.

| Command | Runs |
|---------|------|
| `make quality` (= `lint`, `check`, `tests/scripts/check-quality.sh`) | `tests/scripts/rust-check.sh quality` (after the container isolation check): `check-batches.py`, `check-repository.py`, `triggers.py`, `docs-sync.py`, actionlint, zizmor, gitleaks on the git history, typos, `taplo fmt --check`, Biome, rumdl, hadolint, `buf lint`, `shellcheck tests/scripts/*.sh tools/scripts/*.sh tools/build-scripts/*.sh`, Python unittest, flake8, SwiftLint, detekt, `cargo fmt --check`, `cargo shear`, clippy `-D warnings`, `cargo doc` with `RUSTDOCFLAGS=-D warnings` (no tests: `make test`) |
| `make lint-android` | `tests/scripts/rust-check.sh android-lint`: `check-repository.py` and detekt, the Android part of `make quality`; both run even if the first fails |
| `make fmt` | `cargo fmt --all -- --check` (check only) |
| `make coverage` (= `tests/scripts/check-quality.sh --coverage`) | cargo-llvm-cov with the line coverage ratchet: total ≥ baseline, changed lines ≥80% |
| `make security` (= `audit`, `deny`) | `rust-check.sh security`: `cargo deny --frozen check advisories bans licenses sources` (offline, the advisory DB from the image) |
| `make doc` | `cargo doc --workspace --no-deps` |
| `make test-all` | `docker-test.sh all`: build, quality, test, coverage, security, network, UI tests |

### Check output

`python3 -B tools/scripts/run-check.py <check-id> -- <command> ...` provides the
shared quiet contract. Success prints only `OK <check-id>`. Failure preserves
the exit status (including signals) and prints `FAIL`, a `FIX` documentation
anchor, then `DETAILS` with selected diagnostics and a raw-log pointer.
`tools/scripts/check-output.json` maps IDs to owning documentation and diagnostic
formats; repository validation checks its schema, regular expressions and links.

The parser keeps a bounded first/tail copy in memory; every run also retains
full captured output under `build/check-output/` with a unique filename.
Use `--log-dir <writable-path>` before the check ID to select another directory.
Unrecognized failure output still yields the original nonzero exit status and
a raw-log pointer. Logs are not automatically removed.
The quality/security commands in `rust-check.sh` use this wrapper. JSON report
files remain available; multiline failed-Cargo JSON errors select diagnostics
without passing compilation progress. Native CI archives raw logs as
`quality-check-output` and `core-check-output`; local logs remain in the target
volume. Core repository, batch, docs-sync, trigger-validation and panic/doc-budget CLIs
also use concise defaults. Add `--raw` for their previous machine output; quality
passes this flag to its enclosing wrapper. Trigger diff arguments retain their
existing data API, and help passes through. Imports remain quiet. Standalone
frontends use `DYAPP_CHECK_LOG_DIR` or a writable temporary `ai/check-output`
location, so read-only checkouts work. JUnit, XCTest, Android instrumentation and
coverage validators also use this default and accept `--raw` for their previous
output. Coverage reporting explicitly selects raw metrics; validation thresholds
and JSON/XML report files are unchanged. Android retains validator logs under
`/reports/<container>/check-output`; Apple CI includes them in
`apple-results/check-output`. The ELF dependency auditor also defaults to
`OK auditable` or `FAIL`/`FIX`/`DETAILS`, accepts `--raw`, and retains failed
diagnostics under `DYAPP_CHECK_LOG_DIR` (default `/tmp/ai/check-output`). Its
frontend is self-contained because the release image copies only that script;
a test binds its fix pointer to the shared map. Release artifact-size,
measurement, reproducibility, Android package and Apple package/producer CLIs
also dispatch through the map, including argument and startup failures. Their
`--raw` mode retains direct diagnostic output; JSON reports remain files consumed
by the nightly size/reproducibility workflows. Recognized structured failures
contribute only their `DETAILS` body, without repeated headers or progress.
Nested wrappers preserve one recognized child failure frame when its `FIX`
resolves through the central map; they retain their own raw-log pointer and
original process status. Unmapped child metadata does not become a top-level
fix pointer, and termination signals still identify the owning process.
Characters unsupported by the terminal encoding are escaped for display;
retained raw bytes and the child exit status remain unchanged.
Platform test frontends follow this contract. The [entry-point inventory](check-output.md)
covers Make targets, scripts and every workflow, including explicit report/data
consumers and internal stage ownership.

### Commit messages

`tests/scripts/check-repository.py` (in `make quality` and CI `quality`) enforces practice A3 of
[practices.md](practices.md) on every non-merge commit after a baseline commit (`BASELINE` in the
script; older history is grandfathered):

- subject at most 72 characters, capitalised, imperative (no `-ed`, `-ing` or third-person `-s`
  first word after an optional `scope:` prefix and its space), no trailing period;
- a blank line between the subject and the body;
- a `Co-Authored-By: Name <email>` trailer (not required from `[bot]` authors such as Dependabot);
- no tracker IDs.

Tracker IDs (`dyapp-` followed by 3–4 lowercase letters or digits, optionally `.N`) are also
rejected in every tracked text file. Names declared as `name = "dyapp-…"` in a tracked
`Cargo.toml` are allowed, as is the stale `dyapp-core` in `build-uniffi.sh` (`NOT_TRACKER`).
The check reads committed history, so a bad message fails the next `make quality`; fix it before
pushing. The published-history cutoff was moved to `834083c` on 2026-10-23 because that commit
had already reached `main` with a lowercase subject. Commits after it still follow the same
rules. A clone without the baseline commit (shallow) fails the check.

### Flow metrics

`tools/scripts/flow_metrics.py` provides deterministic A7 calculations over local snapshots.
It measures seven-day UTC cohorts: created-to-closed lead hours (nearest-rank p50/p85),
closed leaf-work throughput, current WIP by assignee (the agent identity), canonical
`ci.yml` main red intervals and recovery hours, revert and escape-hatch shares,
and summed job execution minutes clipped to the week. WIP above 3 or red time above
24 hours produces errors; empty rate/percentile samples are `null`. Older red periods
retain their original start, and late results from old commits cannot clear newer failures.
False positives require explicit boolean observations; four-week review due dates use
adoption dates and explicit review records. Run the local read-only
[`flow-report.py` command](flow-metrics.md) weekly; its JSON report and saved input
support offline reproduction without querying beads from CI.

### Small batches

`tests/scripts/check-batches.py` (first in `make quality` and CI `quality`) enforces A4.
Every non-merge commit after `[batches].base` in `tests/scripts/baselines.toml` must change at
most `max_lines = 400` text lines, additions plus deletions, or carry a nonempty
`Large-Change: <reason>` in the final commit metadata paragraphs. It may be separated
from `Co-Authored-By` by a blank line; the reason must be nonempty. Split an oversized
commit or give its reason before creating it. The fixed baseline excludes the already
published history; there is no growing SHA allow-list. Exactly 400 passes. Renames count as deletion
plus addition; binary files contribute no text lines. Commits on merged branches are checked,
but merge commits themselves are excluded, as in the commit-message check.

Excluded paths are lockfiles (`*.lock`, `*.lockfile`, `package-lock.json`,
`npm-shrinkwrap.json`), snapshots (`*.snap` or a `snapshots/` or `__snapshots__/`
path component), and generated code (`*.uniffi.rs` or a `generated/` path component).
Other generated files must be added to the explicit exclusion policy before they are exempt.

The script requires complete history and an ancestor baseline; missing history fails.
The `commit-msg` hook checks staged lines against the proposed message before the commit;
`make quality` and the `pre-push` hook also check committed history. Run the history check
before pushing if hooks are unavailable.
`python3 -B tests/scripts/check-batches.py --base <commit> --head HEAD --report <path>`
reproduces a range. Its JSON report lists each commit's count and exception reason and
the percentage of large commits, including justified ones. The target is ≤10%; this is
an observed metric, not an additional veto of a justified exception. Weekly escape-hatch
follow-up follows [the shared practice rules](practices.md#rules-shared-by-all-practices).
CI uploads `build/batches.json` as `commit-batches`, also on a failed check.
The published-history baseline was moved to `834083c` on 2026-10-23 after `c0cd328` reached
`main` with 485 counted lines and no trailer. This one-time cutoff preserves the published
history but stops checking commits through that point; moving it again would further weaken
the check. The required `commit-batches` PR status runs the checker from the base branch over
every proposed commit. For non-owner writers, `main` requires a PR and allows only rebase merge,
so squash cannot turn several compliant commits into one oversized commit. Account `abbat` has
an `always` ruleset bypass and can push directly to `main`; those commits get a post-push CI
check. A server-side pre-update gate for owner pushes is planned. Until it exists, keep the local
`pre-push` hook enabled. The owner also reviews changes to
`.github/workflows/batch-policy.yml`: a PR can edit its own workflow.

### Docs change with the code

`tests/scripts/docs-sync.py` (in `make quality` and CI `quality`) enforces practice A2: every non-merge
commit after its `BASELINE` that changes an interface line also changes a file under `docs/`.
Interface lines (`RULES` in the script, a heuristic):

- config keys: `pub` fields in `rust/bootstrap/src/config.rs`; `dyappd` derives its
  `--section.key` flags from them;
- CLI flags: `"-…"` strings in `rust/bootstrap/src/bin/*.rs`;
- public API: `pub fn|struct|enum` and `uniffi::` lines in `rust/ffi/src/`, and
  `message`/`enum`/`service`/`rpc` and field-number lines in `proto/*.proto`;
- Make targets: `target:` lines in the `Makefile`.

For a false positive (say, a renamed private config field), add a `Docs-Unchanged: <reason>`
trailer to the commit message. Like the message check, the script reads committed history, so
fix a failing commit with `git commit --amend` or a rebase.

### Size budgets

`tests/scripts/check-repository.py` (in `make quality` and CI `quality`) enforces practice B6: a tracked
source file (`SOURCE` suffixes in the script: Rust, Python, shell, Kotlin, Swift, TypeScript,
JavaScript, protobuf) has at most `[size] max_lines` (1000) lines. `[size.files]` of
`tests/scripts/baselines.toml` is empty: every file is within the limit. A file listed there would keep
its line count as a budget that fails when it grows and also when it shrinks, so the budget is
lowered in the same change (or the entry dropped once the file is within the limit). Adding an
entry, raising a budget or raising `max_lines` weakens the check.

### Trigger list

`tests/scripts/triggers.toml` is the machine-readable list of practice A6a of
[practices.md](practices.md): changes that need extra review. Each trigger has fnmatch `paths`
(`*` also matches `/`) and, optionally, a `lines` regex; then it fires only when an added or
removed line of a matching file matches. The triggers are:

- `crypto`: encryption modules, proof-of-work, endpoint tags;
- `crypto-dependency`: a cryptographic crate line in a `Cargo.toml`;
- `identity`: the `identity` and `profile` crates and their `.proto` files;
- `protocol`: `proto/`, the wire format, protocol IDs, the UniFFI interface;
- `storage-migration`: a `CREATE`/`ALTER`/`DROP` `TABLE`/`INDEX` or `user_version` line in Rust;
- `unsafe`: an `unsafe` line in Rust;
- `network-surface`: `p2p-net` and the bootstrap node, service, rate limits, deny list,
  replication and configuration;
- `checks`: workflows, scripts, tests, Docker images, lint and deny configurations, and this list.

`python3 tests/scripts/triggers.py` (in `make quality` and CI `quality`) fails when a glob matches no
tracked file, so a moved or deleted path cannot silently drop out of the list. With `git diff`
arguments it also prints the triggers a change fires, one `trigger: path` per line:

```sh
python3 tests/scripts/triggers.py origin/main...HEAD   # a branch
python3 tests/scripts/triggers.py --cached             # staged changes
```

The detector only reports; the review a fired trigger requires (a quorum, a threat model update)
is planned as practices A6 and C1. Removing or narrowing an entry weakens a check, and editing the
list fires `checks` itself.

### Git hooks

`.pre-commit-config.yaml` defines fast local hooks:

- `pre-commit` stage: `check-fast-precommit.py` checks staged whitespace and that Rust integration-test subprocesses using `.env_clear()` forward `LLVM_PROFILE_FILE` for coverage. It reads staged blobs, not unstaged edits.
- `commit-msg` stage: `check-repository.py --message-file` checks the proposed message, and `check-batches.py --staged --message-file` checks the staged line count and any `Large-Change` reason before a commit exists.

Install with `pre-commit install --hook-type pre-commit --hook-type commit-msg` when
`core.hooksPath` is unset. In the maintainer checkout, `core.hooksPath` points to
`.beads/hooks`: its `pre-commit` hook runs the same fast staged check before Beads,
its `commit-msg` hook checks the proposed message and batch size, and its `pre-push`
hook checks committed history on a direct push to `main`.
Full Docker quality, tests, coverage, rustfmt and platform checks run in CI; use
`make quality` or `make test-all` explicitly when local results are needed. Git
hooks can be bypassed with `--no-verify`; server-side batch enforcement remains
planned.

## Rust lints

Gate: `cargo clippy --workspace --all-features --all-targets --locked --offline -- -D warnings`,
i.e. clippy's default lint groups as errors. The unsafe policy and explicit panic
budgets below add targeted rules. Other pedantic/restriction lints (such as
`arithmetic_side_effects` and `cast_possible_truncation`) are not generally enforced.
`codeql.yml` additionally reports `clippy::pedantic` without failing; see
[linting.md](linting.md#rust).
A local `#[allow]` states its lasting reason, in `reason = "..."` or in the comment beside it
(generated modules); none is temporary. This is a review convention, not a check.

### Explicit panic budgets

Practice B8 runs `tests/scripts/check-panics.py` in `make quality` and CI after the
normal Clippy gate. It checks all-feature workspace libraries, binaries and their
build scripts, excluding test targets, dependencies and generated files under
Cargo's target directory. Clippy `unwrap_used`, `expect_used` and `panic` use
`--force-warn`, so an `allow` attribute cannot hide a counted site.
Repeated diagnostics for the same package/lint/source span count once.

`[panics]` in `tests/scripts/baselines.toml` records each crate's budget. FFI must
stay at zero; other crates, including p2p-net, initially ratchet their existing
sites. Growth fails; shrinking requires lowering the budget in the same commit.
A missing package/budget or failed/incomplete Clippy run fails rather than
reporting zero. Details are saved in `build/panics.json` (CI artifact
`panic-budgets` when this check ran). Platform crates outside the workspace are
not covered. These three lints do not prove runtime panic freedom: indexing,
assertions, allocation failure and transitive dependency panics are outside this check.

The budget is the barrier, not `deny`: an `allow` cannot hide a counted site, so a new
production `unwrap`/`expect` fails `make quality` unless one is removed in the same commit.
Unit tests (`#[cfg(test)]`), integration tests, fuzz targets and generated protobuf modules
are not counted. The counted sites, inventoried on 2026-10-23, fall into these groups. None
is reachable by peer input, which `tests/fuzz` also exercises:

- **Validated input.** A record or manifest the service has already accepted: a verified
  signature implies a 32-byte key, an accepted mailbox put decodes, and a stored manifest has
  `data_shards()` (bootstrap `node/replicas.rs`, `node.rs`, `media_repair.rs`,
  `service/media.rs`).
- **State-machine invariants.** An entry looked up right after it was inserted or checked
  (`node/replicas.rs`, `search/find.rs`, `search/generation.rs`).
- **Poisoned locks.** `lock().unwrap()` re-raises a panic that already happened on another
  thread (`rate_limit.rs`, `maintenance.rs`, messaging `hlc.rs`).
- **Constants and startup.** Nonzero or in-range constants, valid Argon2, HMAC and
  gossipsub parameters, and a missing OS random source, where failing closed is the intended
  behaviour (`config.rs`, `lib.rs`, `secret.rs`, identity, profile, p2p-net).
- **Wall clock before 1970.** `SystemTime::duration_since(UNIX_EPOCH)` (messaging, video).

New code returns `Result` for anything a peer or user controls. A new invariant replaces a
removed site, or raises the budget with the reason in review.

### Public API documentation

Practice B9: `missing_docs = "deny"` is set in `[workspace.lints.rust]` of the root
`Cargo.toml` and in the own `[lints.rust]` of `bootstrap` and `ffi`, so an undocumented
public item (crate, module, type, field, variant, function or constant) fails Clippy,
build and test. Generated code is exempt only where its generator emits no docs, each with
a module-level `#![expect(missing_docs, reason = "...")]` and a comment: the prost modules of
`identity`, `profile` and `p2p-net`, and the private `behaviour` module of `p2p-net`, whose
`NetworkBehaviour` derive emits `BehaviourEvent`. The expectation fails quality once the
generated item gains documentation and the exemption becomes unnecessary.

### Unsafe policy

`unsafe_code = "forbid"` applies to every crate except two:

- workspace members via `[workspace.lints.rust]` in the root `Cargo.toml` and `[lints] workspace =
  true`; `clients/linux/`, `clients/windows/`, `tools/arch-lint` and `tests/fuzz/` set it in their own `[lints.rust]`;
- `rust/bootstrap` (libc calls) and `rust/ffi` (uniffi generates `#[no_mangle]`) instead deny
  `clippy::undocumented_unsafe_blocks`: each `unsafe` block needs a `// SAFETY:` comment stating
  the invariant that makes it sound.

`tests/scripts/check-repository.py` fails when a crate's `Cargo.toml` lacks its policy, so a new crate
cannot skip it. Adding `unsafe` code is a change trigger (see [Trigger list](#trigger-list)).

Formatting: rustfmt defaults (edition 2021, no `rustfmt.toml`).

### Secret and crypto hygiene

Practice C6. Secret material lives only in wrappers that clear it on drop and never print it:

| Secret | Holder | `Debug` |
|--------|--------|---------|
| Ed25519 identity and device keys | `dyapp_identity::Identity` (ed25519-dalek's `SigningKey`, zeroized on drop); `Identity::secret` hands out `Zeroizing<[u8; 32]>` | `Identity("<peer id>")` |
| node key file | read into and written from `Zeroizing` buffers (`config.rs`) | libp2p prints the public key only |
| coturn `static-auth-secret` (`turn.secret`) | `dyapp_bootstrap::secret::Secret` (`Zeroizing<String>`); `Secret::hmac_sha1` derives the TURN passwords | `Secret(redacted)` or `Secret(unset)` |

Secrets and nonces are compared only with `subtle::ConstantTimeEq`: `Secret`'s `PartialEq` and
the mailbox challenge check (`fresh` in `service.rs`). Content hashes, peer IDs and signatures are
public and compare with `==`. Unit tests in `secret.rs` and `rust/identity` assert that `Debug`
shows neither the secret nor its bytes.

`clippy.toml` bans, with the reason in each entry, `std::mem::forget` (it skips the zeroizing
`Drop`), `ed25519_dalek::SigningKey::to_bytes` (an unzeroized copy of the key) and the type
`sha1::Sha1`. A site that needs one says why with
`#[expect(clippy::disallowed_types, reason = "...")]`; the only one is `Secret::hmac_sha1`, since
coturn's REST credentials are HMAC-SHA1. The clippy step of `make quality` and CI enforces the
list. Planned: the messaging and video encryption are placeholders without real key material;
they get the same wrappers when they carry keys.

## Dependency checks (cargo-deny)

`deny.toml`, run by `rust-check.sh security`. Under `make security` it runs offline against the advisory
databases baked into the dev image. In the CI `core / build` step `deny` it fetches them at run time:

- **advisories**: any RustSec advisory fails (no CVSS threshold); a yanked crate fails. Ignored: RUSTSEC-2024-0436
  (`paste` unmaintained, a proc-macro reached only through libp2p's `if-watch`, no upgrade yet),
  RUSTSEC-2026-0374 (`basic-toml` unmaintained, a build-time parser reached only through uniffi's
  `askama`, no upgrade yet) and RUSTSEC-2026-0173 (`proc-macro-error2` unmaintained, a proc-macro
  of `hax-lib` reached only through openmls -> hpke-rs -> libcrux, no upgrade yet), and
  RUSTSEC-2026-0330 and RUSTSEC-2026-0331 (a panic on a short hybrid seed or key in
  `libcrux-kem` 0.0.9, pinned by hpke-rs 0.7, which openmls needs; only hybrid KEMs are hit and
  dyapp accepts ciphersuite 0x0001 only). `cargo audit` in `security.yml` ignores the same
  vulnerabilities through `.cargo/audit.toml`, and the Scorecard Vulnerabilities check ignores all of
  these through `osv-scanner.toml`.
- **licenses**: only the listed licenses are allowed (permissive set plus `MPL-2.0`); anything else fails.
- **bans**: `openssl` and `openssl-sys` are denied; a second version of a crate fails unless
  `[bans] skip` lists that older version with the dependency that keeps it. cargo-deny warns about
  a skip that no longer matches; remove it then.
- **sources**: crates.io only; an unknown registry or git source fails.

To accept an advisory, add its ID to `[advisories] ignore` with a comment giving the reason and an
issue to remove it. That hides the advisory; it does not fix it.

### Advisory ignores

Every advisory ignore has a review date at most 90 days ahead, so no acceptance is forgotten:

- `deny.toml`: the reason ends in `review by YYYY-MM-DD (issue #N)`;
- each `osv-scanner.toml`: the entry sets `ignoreUntil = YYYY-MM-DD`, after which OSV-Scanner
  reports the vulnerability again;
- `.cargo/audit.toml` has no reasons, so it ignores exactly the `deny.toml` ids.

`tests/scripts/check-advisory-ignores.py` checks this in `make quality`, but never fails on a passed
date, so time alone cannot turn a commit red. The scheduled `security.yml` run adds `--expiry`: a
date that has passed or is less than 14 days away fails the job `review-dates`, and nightly-health
opens a tracked issue. To clear it, review the ignore: drop it if the fix landed, or record the
current blocker and move the date.

### Dependency age

A hijacked crate is usually found and yanked within days, so no crates.io version younger than
14 days may enter a tracked `Cargo.lock`. `tests/scripts/check-dependency-age.py` runs in
`make quality` and reads each version's `pubtime` from the registry index cache: the dev image
and the CI `fetch` step run `cargo fetch` for every lockfile. A version missing from the cache
fails; rebuild the dev image. A version recorded before crates.io kept `pubtime` counts as old.
Dependabot waits the same 14 days (`cooldown` in `.github/dependabot.yml`).

To take a younger version, for example a security fix, add `(crate, version): (date, reason)` to
`EXCEPTIONS` in the script. The `ctutils 0.4.4` exception, added on 2026-10-23 for
`RUSTSEC-2026-0385`, allows the fixed version immediately. An exception older than 14 days
fails the check: by then the version is old enough, so remove the entry.

## Source debt markers

B7 is enforced by `check-repository.py` in `make quality` and CI. Uppercase
`TODO` and `FIXME` words are rejected in source comments and string literals,
with file/line diagnostics and no allow-list. The inventory includes tracked
and non-ignored new files in Rust, Python, shell, Kotlin, Swift, Java, JS/TS,
protobuf, HTML/CSS, C/C++/Objective-C, Gradle and PowerShell. Markdown and data
files are outside this source check; larger identifiers containing those words
are not markers. Rust quality additionally denies `clippy::todo` for every
workspace target, rejecting the executable `todo!()` macro.

Track deferred implementation in local beads; retain factual limitations in
source and owning docs. Moving a marker does not complete its implementation.
The messaging encryption/signature/key-derivation APIs still contain explicit
placeholders; their replacement remains tracked separately under the
[MLS design](../decisions/0005-openmls-end-to-end-encryption.md).

## Hidden Unicode

`check-repository.py` also refuses, with file and line, Unicode format characters (category Cf:
bidi overrides and isolates, zero-width characters) and private-use characters (Co) in every
tracked text file, and any tracked path with a non-ASCII character. A zero-width joiner between
two symbols (an emoji sequence) is allowed. Swift and Kotlin sources must be ASCII only: their UI
text lives in string catalogues, and neither SwiftLint without SourceKit nor detekt can check
identifiers for lookalike letters. Cyrillic, CJK and emoji stay allowed elsewhere. Rust
identifiers are covered by rustc and clippy (`confusable_idents`, `mixed_script_confusables`).
`tests/test_unicode.py` covers the check.

## Android manifest

`check-repository.py` refuses a tracked `AndroidManifest.xml` whose application does not set
`allowBackup="false"` and `dataExtractionRules`, or that exports any component other than the
launcher activity. The rules in `res/xml/data_extraction_rules.xml` exclude every domain from
cloud backup and device transfer. `tests/test_android_manifest.py` covers the check.

## Coverage

Threshold, metric, gate locations and the (non-enforced) per-crate targets are defined once in
[Coverage policy](../testing/README.md#coverage-policy): the workspace line total never drops
below the baseline in `tests/scripts/baselines.toml`, lines changed since the base are ≥80% covered, and
there is no per-crate gate. `make coverage` writes `coverage.json`, `coverage.lcov` and
`coverage.txt` (per-file table); find uncovered files there and add tests for the missing
branches.

## Conventions (not enforced by tooling)

- Public items should have `# Errors` / `# Panics` sections where they apply; only the doc
  comment itself is enforced ([B9](#public-api-documentation)).
- Library code returns `Result` instead of calling `unwrap`/`expect`/`panic!`; tests may unwrap.
- Do not leave `todo!()` or `unimplemented!()` in committed code; open an issue and return an
  error instead.
- There are no benchmarks (`criterion` is not a dependency and there are no `benches/`).
- Compare secrets (keys, MACs, release secrets) in constant time with `subtle::ConstantTimeEq`,
  never `==`; public values such as nonces, hashes and signatures may use `==`.
- Client code (Rust core, FFI and apps) never logs message content, keys or contacts'
  identifiers (peer IDs, mailbox addresses). Types that hold them print none of it in `Debug`
  (`Identity`, `Attachment`, `Event`, `Delivery`, `Received`, `MlsReceived`, `Message`), with a
  test each. The clients have no logger and no crash reporting; one added later logs warnings and
  errors only in release builds.
- Apps show data from peers (profile text, names, messages) as plain text: no markdown, HTML,
  autolinks or WebView, and no URL scheme or deep link built from it.
- <a id="sql"></a>**SQLite statements.** Quote every table, column, index and alias name with
  backticks; SQL keywords, functions and pragma names stay bare:
  ``"SELECT `record` FROM `profiles` WHERE `peer_id` = ?"``. Backticks keep column names that
  are SQLite keywords (`attachments`.`release`) safe, and one style keeps the queries greppable; SQLite
  accepts them as identifier quotes, as it does the ANSI double quotes, which would need escaping
  in Rust strings. Bind every runtime value as a parameter (`?`, `?1`); never `format!` a value
  into SQL. A `format!` fragment may only be an identifier or clause from an in-code constant
  (`&'static str`), or an integer the code computes where SQLite has no parameter (a `DEFAULT`
  in DDL, a pragma argument). Constant literals written in the SQL itself are fine.

## CI

See [CI matrix](ci-matrix.md) and [CI base](ci-base.md). Job `quality` in `ci-base.yml` runs
`make quality`'s container; job `build` runs build, test, coverage, security and network. Required status checks
are **not established**: see [Required checks](ci-matrix.md#required-checks).

## References

- [Clippy lints](https://rust-lang.github.io/rust-clippy/)
- [cargo-deny](https://embarkstudios.github.io/cargo-deny/)
- [RustSec Advisory Database](https://rustsec.org/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
