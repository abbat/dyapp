# Engineering practices

Catalogue of development practices that dyapp adopts so that AI agents follow established,
measurable rules instead of improvising. Every practice names an external standard, a rule an
agent can execute, a metric with a threshold and a tool. All practices below are **planned**;
when one is adopted, append `(adopted)` to its id and describe it on the page that owns the
check ([code-quality.md](code-quality.md), [linting.md](linting.md), [testing](../testing/README.md), [security](../security/threat-model.md)).

## Human gates

A human is required only for:

- **priorities**: the order of epics (A11 prepares a proposal);
- **UX decisions**: approval of screen changes (D3);
- **releases**: the final go (A10), including approval of accepted risks (C1, C2).

Architecture and security decisions go through a [quorum](quorum.md) instead of a human (A6).

## Scoring

| Scale | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| **U**, usefulness for dyapp | not applicable, excluded | nice to have | closes a known risk | closes a risk that has occurred or is critical for a messenger |
| **F**, feasibility by agents | impossible, excluded | human needed at each step | agent does, human accepts | fully automatic (CI or script) |
| **C**, cost (agent time, CI minutes, money) | — | ≤ 1 day, CI time +10% or less | several days or a separate nightly job | weeks, recurring manual work or money: **deferred** |

Score = U × F / C. Waves: **W1** score ≥ 4.5; **W2** score 2–4.4, or tied to a later trigger;
**W3** C = 3, deferred.

Automation: `auto` (CI or script), `semi` (agent produces, quorum or check verifies),
`human-gate` (agent prepares, human approves), `human`.

Lifecycle stages: intake, design, implement, verify, release, learn.

## Rules shared by all practices

- Beads are internal: bead IDs never appear in docs, code, comments or commit messages (A3).
  External references go to GitHub Issues, `docs/` paths and ADRs. Postmortems (A8) stay in beads;
  only conclusions reach docs or ADRs.
- **CI never touches beads.** It fails with a machine-readable report (JUnit, JSON artifact or
  SARIF in code scanning); the agent turns reports into beads locally (A12). Checks over beads
  (A1, A7) are local scripts.
- A check that cannot run is a failure, never a skip. An external service that fails with a
  network error is retried twice, then the check fails.
- **Adoption is a ratchet.** A practice is adopted as a check that blocks getting worse than the
  current count; reaching its target threshold is separate, small beads. Ratchets store their
  baseline in the repository.
- **CI budget.** The push pipeline takes ≤ 20 minutes wall-clock; heavier runs (full B2, B4
  campaigns, full D2) are nightly. A7 tracks the budget.
- **Escape hatches** are commit trailers or annotations with a reason: `Large-Change:`,
  `Size-Increase:`, `Docs-Unchanged:`, `#[mutants::skip]`, a flaky-test quarantine.
  - *Observed* hatches are applied by the agent and counted in A7; more than 10% of commits in a
    week triggers A8.
  - *Protected* hatches need a closed [quorum](quorum.md): lowering any baseline, `Quorum:
    not-needed`, and any hatch in an A6a trigger path.
  - The A10 checklist lists every hatch used since the previous release.
- **Checks are protected code.** A change that weakens a check (workflows, check scripts,
  baselines, `clippy.toml`, `deny.toml`, allow-lists, the A6a trigger list) is an A6a trigger;
  tightening one is not. A new tool is pinned (action SHA, `cargo install --locked --version`,
  npm lockfile) and installed only in the Docker images.
- **Residual risk.** A quorum may mark a threat-model cell or control (C1, C2) `mitigated` with a
  test or `open` with a bead; it only proposes `accepted`, which a human approves in the A10
  checklist. Message confidentiality (end-to-end encryption, keys) is never `accepted`.
- **Feedback.** Four weeks after adoption the agent reviews each practice from A7 data (defects
  caught, false-positive share, CI minutes, hatches used) and proposes keep, tighten or retire.
  A heuristic check with > 20% of its hits closed by a hatch over four weeks is revised. A retired
  practice moves to [Excluded](#excluded) with its data; retiring a C practice needs a quorum.
  Before W2 starts, U/F/C scores are revised from W1 data and reviewed by a quorum.

## A. Agent process and flow

| id | Practice and rule | Stage | Standard | Metric, threshold | Tool | Auto | U/F/C | Wave |
|---|---|---|---|---|---|---|---|---|
| A1 (adopted) | **Definition of Ready.** A task, feature or bug bead has a goal, ≥ 1 verifiable acceptance criterion, the affected docs (or "none") and a verification method before it is claimed | intake | INVEST, Scrum DoR | 100% of claimed beads pass | [`bead-ready.py`](../agents/beads.md#definition-of-ready) over `bd show --json`, run before a claim | auto | 3/3/1 | W1 |
| A2 (adopted) | **Docs change with the code.** A diff that changes a public API, CLI flag, config key or Make target without a change under `docs/` fails | verify | Docs-as-Code | 0 such commits | `docs-sync.py` (heuristic, `Docs-Unchanged:` trailer for false positives) | auto | 3/2/1 | W1 |
| A3 (adopted) | **Commit lint and no tracker leak.** Imperative subject ≤ 72 chars, `Co-Authored-By` trailer; no bead ID (`dyapp-[a-z0-9]{3,4}(\.\d+)*`) in tracked files or commit messages, package names from `Cargo.toml` (such as `dyapp-ffi`) allow-listed | verify | Chris Beams' seven rules | 0 violations on `main` | script or gitlint in CI | auto | 2/3/1 | W1 |
| A4 (adopted) | **Small batches.** A commit changes ≤ 400 lines excluding lockfiles, snapshots and generated code, or carries `Large-Change: <reason>` | implement | Google Eng Practices "Small CLs" | ≤ 10% large commits | script in CI | auto | 2/3/1 | W1 |
| A5 | **ADR triggers.** A new direct dependency, new crate, wire-format or storage-schema change carries `ADR: docs/decisions/...` | design | Nygard ADR, MADR | 100% of triggers referenced | manifest diff script | auto | 3/3/1 | W1 |
| A6a (adopted) | **Trigger list.** A machine-readable list of trigger paths (cryptography, identity, protocol, storage migrations, `unsafe`, new network surface, and the checks themselves) and a detector that reports which triggers a diff fires | design | NIST SSDF PW.7 | detector covers 100% of listed paths | path list + script | auto | 3/3/1 | W1 |
| A6 (adopted) | **Quorum triggers.** A change that fires an A6a trigger needs a closed quorum before merge | design | two-person rule, NIST SSDF PW.7 | 100% of triggered changes | A6a detector + checklist | semi | 3/2/2 | W2 |
| A7 (adopted) | **Flow metrics.** Weekly report: bead lead time p50/p85, WIP, throughput, red-commit rate on `main`, time to green, revert rate, hatches used, CI minutes, false-positive share per heuristic check. Thresholds: `main` red ≤ 24 h, WIP ≤ 3 per agent | learn | Kanban, DORA (pre-release adaptation) | thresholds in the rule | local script over `bd`, `git log` and `gh run list` | auto | 2/3/1 | W1 |
| A8 | **Blameless postmortem.** On `main` red > 24 h, a regression in a release or a security advisory, the agent writes an `analyse` review with action items as beads | learn | Google SRE postmortem | 100% within 48 h | `analyse` + beads | auto | 2/3/1 | W1 |
| A9 (adopted) | **Docs and instruction integrity.** No broken links, every file under `docs/` is reachable from the [index](../README.md), `AGENTS.md` ≤ 80 lines | verify | Diátaxis | 0 broken links and orphans | `check-repository.py` (relative links and anchors; external links are not checked) | auto | 2/3/1 | W1 |
| A10 | **Release process.** SemVer, changelog from commits, checklist (all gates green, SBOM, provenance from C9); a human gives the final go | release | SemVer 2.0, Keep a Changelog | no release without the checklist | git-cliff + checklist script | human-gate | 2/3/1 | W2, before the first release |
| A11 (adopted) | **Priority proposal.** The agent ranks open epics by WSJF (cost of delay / size) and proposes the order; a human approves | intake | WSJF, RICE | refreshed every 2 weeks | script + `bd` | human-gate | 2/2/1 | W2 |
| A12 (adopted) | **CI report import.** A local script reads failing CI jobs (`gh run view --log-failed`) and creates or updates beads without duplicates | learn | — | 100% of failing reports have a bead | [`ci-report-import.py`](../agents/beads.md#ci-failures) + `bd` | auto | 3/3/1 | W1 |

## B. Hidden quality debt

| id | Practice and rule | Stage | Standard | Metric, threshold | Tool | Auto | U/F/C | Wave |
|---|---|---|---|---|---|---|---|---|
| B1 (adopted) | **Coverage ratchet.** Total line coverage never drops below the stored baseline; changed lines are ≥ 80% covered | verify | Google "Code Coverage Best Practices" | total ≥ baseline, diff ≥ 80% | cargo-llvm-cov (lcov) + `scripts/check_coverage.py` | auto | 3/3/1 | W1 |
| B2 (adopted) | **Mutation testing.** cargo-mutants `--in-diff` on every push: 0 missed mutants, exceptions only via `#[mutants::skip]` with a reason. Weekly full run with a mutation-score ratchet | verify | mutation testing (PIT, Stryker) | diff: 0 missed; full ≥ baseline | cargo-mutants ([mutation testing](../testing/README.md#mutation-testing)) | auto | 3/3/2 | W1 |
| B3 (adopted) | **Round-trip properties.** Every wire and storage type has a proptest `decode(encode(x)) == x`; a script finds types without one | implement | property-based testing | 100% of protocol and storage types | proptest + [`check-repository.py`](../testing/README.md#round-trip-properties) | auto | 3/3/1 | W1 |
| B4 (adopted) | **Fuzzing untrusted input.** cargo-fuzz targets for bootstrap protocol frames, message decoding and profile parsing. CI smoke 60 s per target, nightly 1 h; corpus in the repository; a crash becomes a bead and a regression test | verify | OSS-Fuzz, ClusterFuzzLite | 100% of network parsers have a target; 0 open crashes | cargo-fuzz ([fuzzing](../testing/README.md#fuzzing)) | auto | 3/3/2 | W1 |
| B5 (adopted) | **Flaky tests.** nextest with `retries = 2`; a test that passes only on retry is flaky, gets a bead and ≤ 7 days of quarantine. Same for XCUITest and Android UI tests | verify | Google "Flaky Tests" | 0 flaky tests older than 7 days | cargo-nextest (JUnit) + [`check-flaky.py`](../testing/README.md#flaky-tests) | auto | 2/3/1 | W1 |
| B6 (adopted) | **Size budgets.** A source file ≤ 1000 lines; older large files (`[size.files]` in `scripts/baselines.toml`) do not grow and have beads to split them | implement | Rust API Guidelines, clippy `too_many_lines` | 0 ratchet violations | `scripts/check-repository.py` | auto | 2/3/1 | W1 |
| B7 (adopted) | **No TODO or FIXME in code.** Debt lives in the tracker; existing ones in `messaging/src/encryption.rs` move to beads | implement | — | 0 | script + clippy `todo` | auto | 2/3/1 | W1 |
| B8 (adopted) | **No panics across FFI.** clippy `unwrap_used`, `expect_used`, `panic` for non-test code in `ffi`, then `p2p-net`; other crates ratchet the count | implement | Rust API Guidelines C-GOOD-ERR | `ffi`: 0; others ≤ baseline | clippy restriction lints + counter | auto | 3/3/2 | W1 |
| B9 (adopted) | **Documented public API.** `missing_docs` is denied workspace-wide | implement | Rust API Guidelines C-CRATE-DOC | 0 warnings | rustc lint | auto | 2/3/1 | W1 |
| B10 | **Performance regression.** Hot paths (crypto, codecs, search) are measured in instructions, not time, so CI noise does not matter | verify | — | ≤ +5% over baseline | iai-callgrind | auto | 2/2/2 | W2 |
| B11 | **Deterministic network simulation.** Partitions, delay and loss for `p2p-net` and bootstrap | verify | FoundationDB-style DST | scenarios green | turmoil | auto | 3/2/3 | W3 |

## C. Security assurance

The threat model ([threat-model.md](../security/threat-model.md)), [privacy.md](../security/privacy.md)
and [encryption.md](../security/encryption.md) are the starting point.

| id | Practice and rule | Stage | Standard | Metric, threshold | Tool | Auto | U/F/C | Wave |
|---|---|---|---|---|---|---|---|---|
| C1 | **Living threat model.** A component × STRIDE and LINDDUN matrix; each cell has a status (mitigated / accepted / open) and evidence (a test or file). A change that fires an A6a trigger updates the model in the same commit | design | STRIDE, LINDDUN, OWASP Threat Modeling | 100% of cells have a status; every mitigated cell links a test; 0 open cells without a bead | table check script + quorum | semi | 3/2/2 | W2 |
| C2 | **Verification checklists.** MASVS L2 and MASVS-PRIVACY for Android and iOS; applicable ASVS L2 chapters (V5 input, V9 communications, V14 configuration) for `dyappd`. Mapping file requirement → status → evidence, filled by an agent, reviewed by a quorum | verify | OWASP MASVS, ASVS | 100% of controls have a status; 0 failures without a bead | mapping + completeness script | semi | 3/2/2 | W2 |
| C3 | **OpenSSF Scorecard and Best Practices badge.** Weekly Scorecard run published to code scanning; the badge questionnaire is kept current at every release | verify | OpenSSF Scorecard, OpenSSF Best Practices | Scorecard ≥ 7.0, no regression; badge passing | scorecard-action + bestpractices.dev | auto | 2/3/1 | W1 |
| C4 (adopted) | **CI hardening.** Static analysis of workflows (template injection, persisted credentials, excessive permissions); top-level `permissions:` in every workflow | verify | SLSA Source, OpenSSF | 0 findings | zizmor (offline audits) + `check-repository.py` (top-level `permissions:`) | auto | 3/3/1 | W1 |
| C5 (adopted) | **Unsafe policy.** `unsafe_code = "forbid"` in every crate except `bootstrap` and `ffi` (uniffi emits `#[no_mangle]`); there `clippy::undocumented_unsafe_blocks` is denied and `// SAFETY:` is required; new `unsafe` is an A6a trigger | implement | Rust Unsafe Code Guidelines | 0 `unsafe` outside the allow-list | rustc + clippy + `check-repository.py` (every crate declares its policy) | auto | 2/3/1 | W1 |
| C6 (adopted) | **Secret and crypto hygiene.** Secrets only in `Zeroizing` or `secrecy` wrappers with a redacting `Debug`; secret comparison only via `subtle::ConstantTimeEq`; banned methods and types in `clippy.toml`; a test that `Debug` of secret types reveals nothing | implement | MASVS-CRYPTO, OWASP Cryptographic Storage | 0 violations | `clippy.toml` disallowed-* + tests ([hygiene](code-quality.md#secret-and-crypto-hygiene)) | auto | 3/3/2 | W1 |
| C7 | **Log redaction.** Integration tests capture tracing output and assert no keys, tokens, message content or raw addresses | verify | MASVS-STORAGE-2, OWASP Logging | 0 matches | test subscriber + patterns | auto | 3/2/2 | W2 |
| C8 | **Vulnerability disclosure.** `SECURITY.md`, GitHub private vulnerability reporting, fix SLO by alert age (critical ≤ 48 h, high ≤ 7 days); every fix has a regression test referencing its GHSA or CVE. The SLO is measured automatically; a breach triggers A8; the fix follows the normal flow | learn | ISO/IEC 29147, OpenSSF CVD Guide | SLO met | `gh api` + alert-age script | auto | 2/3/1 | W1 |
| C9 | **Release provenance and signing.** Build attestations; `gh attestation verify` in the A10 checklist | release | SLSA Build L2, then L3 | 100% of artifacts attested | attest-build-provenance | auto | 3/3/1 | W2, with A10 |
| C10 (adopted) | **Reproducible builds.** Double build of `dyappd` and the Linux binary, hashes compared | verify | reproducible-builds.org | hashes equal | nightly CI script | auto | 2/2/2 | W2 |
| C11 | **Supply-chain audits** with imports from Mozilla and Google | verify | cargo-vet | — | cargo-vet | semi | 2/2/3 | W3 |
| C12 | **Formal verification** of protocol parts built on top of MLS | design | — | — | Tamarin or ProVerif, Kani | semi | 2/1/3 | W3 |
| C13 | **External audit or penetration test** before a public release | release | — | — | — | human | 3/1/3 | W3 |

## D. Product and UX

| id | Practice and rule | Stage | Standard | Metric, threshold | Tool | Auto | U/F/C | Wave |
|---|---|---|---|---|---|---|---|---|
| D1 (adopted) | **i18n from day one.** Every visible string lives in resources (`strings.xml`, `.xcstrings`, a JSON catalogue for desktop) with ICU plurals; UI literals are banned; UI tests also run on a pseudo-locale (Android `en-XA`, Xcode double-length) | implement | Unicode CLDR, ICU MessageFormat | 0 literals, pseudo-locale green | [`check-repository.py`](../testing/ui-testing.md#localized-strings) (Compose, SwiftUI and HTML literals) | auto | 3/3/1 | W1 |
| D2 | **Accessibility.** UI tests run XCUITest `performAccessibilityAudit()`, Compose `enableAccessibilityChecks()` and axe-core in the Tauri webview | verify | WCAG 2.2 AA | 0 violations | native audits + axe-core | auto | 3/3/1 | W1 |
| D3 | **UX review checklist.** Every screen change gets a UX bead: screenshots on every platform (light, dark, large font), the platform HIG checklist and a Nielsen heuristics report; a human approves | design | Apple HIG, Material 3, Nielsen's 10 heuristics | 100% of changed screens have an approved UX bead | bead template + D4 screenshots | human-gate | 3/2/1 | W1 |
| D4 | **Screenshot tests.** A screenshot that differs from its committed reference fails; the agent attaches the diff to the D3 bead locally | verify | snapshot testing | 0 unreviewed changes | Roborazzi, swift-snapshot-testing, Playwright | auto | 2/3/2 | W2 |
| D5 (adopted) | **Size budgets.** APK/AAB, IPA, desktop and `dyappd` binaries ratchet; growth > 5% needs `Size-Increase: <reason>` | verify | Android vitals, Apple app size guidance | ≤ +5% without a reason | CI script | auto | 2/3/1 | W1 |
| D6 | **Cold-start budget** | verify | Android vitals, XCTest launch metrics | ≤ +10% over baseline | Macrobenchmark, `XCTApplicationLaunchMetric` | auto | 2/2/2 | W2 |
| D7 | **User journeys as tests.** Every user-facing epic defines 1–3 journeys (such as "send the first message"), each a UI test on every platform | verify | JTBD, BDD | 100% of user-facing epics | platform UI tests | auto | 3/2/2 | W2 |
| D8 | **Shared design tokens.** Colours, spacing and typography in one JSON file, generated for Compose, SwiftUI and CSS | implement | W3C Design Tokens | 0 magic colours or sizes in UI code | Style Dictionary + lint script | auto | 2/2/2 | W2 |
| D9 | **Usability tests with people** | verify | ISO 9241-11, SUS | — | — | human | 3/1/3 | W3 |

Permission and privacy UX (just-in-time permission requests with a rationale) is covered by
MASVS-PRIVACY in C2.

## Excluded

| Practice | Reason |
|---|---|
| Scrum sprints, story points, velocity, retrospective ceremonies | No human team; agent capacity is not estimated. Kanban flow metrics (A7) replace them |
| Waterfall phase gates | Conflicts with the iterative bead flow; DoR, DoD and ADRs give the rigour without phases |
| SAFe, PRINCE2 | Coordination overhead for teams that do not exist |
| Pair and mob programming | The quorum is the equivalent |
| loom | No custom synchronisation primitives, only std `Mutex` and atomics |
| miri | The only `unsafe` blocks are libc FFI calls in `bootstrap`, which miri cannot execute; C5 covers them |
| Product telemetry, A/B tests | Contradicts a private messenger |
| Bug bounty | No budget |

## Adoption

Each practice is one bead, grouped into one epic per area (A–D), with the wave as the priority
(W1 → P1, W2 → P2, W3 deferred). At most three practices are in progress at a time. W1 runs in
four groups, each after the previous one:

1. guards: A3, A6a, A9, A12, C4, C5, A2;
2. ratchets: B1, B6, B7, B8, B9, D5, and A7 limited to hatch counters and CI minutes;
3. tests: B3, B5, B2, B4, C6, D1, D2;
4. process: A1, A5, A7 in full, A8, A4, C3, C8, D3.

The first bootstrap release comes first: D2, D3 and the other open D practices wait until it is
out.

Other dependencies between practices:

- A6a before A6 and C1;
- A12 before B4, B5, D4 and A8;
- A7 before A8 and before the four-week reviews;
- A10 before C9; A7, C1 and C2 before A10, so C1 and C2 come before the first release;
- D4 before D3 attaches screenshot diffs; until then D3 uses manual screenshots.

Adopting a practice means: the check runs in CI or as a script, a failing case was observed, its id
here is marked `(adopted)`, and the page that owns the check describes it.
