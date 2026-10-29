# Security Assurance

How the project measures its own security practices and handles reported vulnerabilities.
The `pr-guard` workflow runs its checker from the trusted base commit and extracts only PR
workflow files for static linting; it does not check out or run the PR tree.

The [exception audit inventory](../development/practices.md#exception-audit-inventory)
lists every repository and GitHub-side waiver source. The
[first review](exception-audit-2026-10-24.md) records evidence and the next
monthly review date; current advisory explanations below are inputs to it.

## OpenSSF Scorecard

Practice C3. `.github/workflows/scorecard.yml` runs
[OpenSSF Scorecard](https://github.com/ossf/scorecard) (scorecard-action 2.5.0) every Monday, after
each push to `main` and on demand:

- job **scorecard - publish** uploads the SARIF result to code scanning (the repository's Security
  tab) and publishes the result to the Scorecard API, which serves the public score. Scorecard
  verifies this job before it accepts the result, so it has no `run` steps and uses only approved
  actions.
- job **scorecard - score** runs Scorecard again with JSON output, and
  `tests/scripts/check-scorecard.py` fails when the aggregate score is below `[scorecard] score` in
  `tests/scripts/baselines.toml`. Below the baseline or the target of 7.0 it lists the checks that
  lose points and any vulnerability IDs; at
  0.5 above the baseline it asks to raise the baseline. Artifact **`scorecard`** keeps the JSON.

The first run (2026-10-19, before `SECURITY.md`) scored 5.9. With `SECURITY.md`,
digest-pinned images, hash-pinned flake8 and the OSV exceptions it rose to 6.8.
The 2026-10-20 run on `fd8c98b` measured 7.3. CI-Tests scores only merged pull requests among
the recent commits; once the last one left that window the check became inconclusive and dropped
out of the average. After the local `tests/docker/Dockerfile.linux-test` was added, Scorecard
counted one more unpinned local base image and the score fell from 7.2 to 7.1.
An early 2026-10-23 run scored 6.5: Vulnerabilities found 13 OSV advisories and scored 0.
After the Android dependency fixes, [the main run](https://github.com/abbat/dyapp/actions/runs/39944202407)
scored 7.3; Vulnerabilities scored 10 with zero findings. The 7.1 baseline and 7.0 target are met.
Maintained remains at 0 while the repository is younger than 90 days; Code-Review and
Contributors are also at 0. The registered Best Practices badge scores 2 and branch
protection scores 4. Improving these checks requires maintainer decisions.

**Exceptions.** Scorecard's Vulnerabilities check runs OSV-Scanner over supported manifests.
OSV-Scanner reads `osv-scanner.toml` from the scanned manifest's directory; a root exception
therefore does not cover a child directory. Each reviewed exception needs a reason tied to its
scan source (no fixed version reachable, a code path the project never takes). cargo-deny keeps
its own list in `deny.toml`. The Linux and Windows client locks link `dyapp-client`, so their
OSV files repeat the workspace's paste, proc-macro-error2, libcrux-kem and crypto-bigint entries;
the Linux one also repeats the lru entry (the desktop node links `dyapp-bootstrap`) and accepts
the unmaintained zvariant, zbus_names and zvariant_derive: secret-service 5.2, the key store
backend, still needs zbus 5, while zbus 6 absorbs those crates.
The `glib` 0.18.5 alert is accepted on Linux until Tauri's GTK 3 stack can use the fixed 0.20
series: the affected `VariantStrIter` API is not used by the app. The Windows alert was dismissed
because this Linux dependency is not compiled for Windows.
Pinned-Dependencies cannot pin the locally built `dyapp:*` base images or a stage of the same
Dockerfile; those findings are dismissed in code scanning as false positives, and so is the
Gradle wrapper JAR under Binary-Artifacts (the checked-in Gradle bootstrap).

**2026-10-23 Scorecard investigation.** The 13 Vulnerabilities findings remained after
reviewing the GitHub alerts because Scorecard runs its own OSV scan. Four Rust advisories
(`RUSTSEC-2026-0374`, `RUSTSEC-2026-0330`, `RUSTSEC-2026-0331`, `RUSTSEC-2026-0173`)
occur in both the root and `tests/fuzz/Cargo.lock`; at the time, only the root
`osv-scanner.toml` covered them. The other nine GHSA IDs match Maven artifacts listed in
`clients/android/gradle/verification-metadata.xml`; that directory had no
`osv-scanner.toml`. The four fuzz exceptions were added after confirming the same dependency
paths and reasons as the root lockfile. The nine Android findings were traced to the Gradle build classpath and verification
metadata; none appears in `:app:dependencies --configuration debugRuntimeClasspath`. The root
`settings.gradle` now substitutes fixed versions of commons-lang3, jose4j, Bouncy Castle and
jdom2 before resolution, as well as the already selected fixed httpclient and Kotlin plugin
versions. The old verification entries were removed and new artifact checksums recorded.
Android CI passed, and the next main Scorecard run reported zero vulnerabilities without new
exceptions. Its score artifact is in the linked run above.
[OSV-Scanner configuration](https://github.com/google/osv-scanner/blob/main/docs/configuration.md)
scopes exceptions to a scan source's directory; [supported manifests](https://github.com/google/osv-scanner/blob/main/docs/supported_languages_and_lockfiles.md)
include Cargo locks and Gradle verification metadata.
The new `RUSTSEC-2026-0385` for `ctutils 0.4.3` was addressed by updating both affected Cargo
locks to the fixed `0.4.4`; the main Scorecard run above confirmed zero findings.
`GHSA-qwgh-2vcv-g2f7` lists every `block-buffer` before 0.12.1, but its corrupted cursor is the
inline one of 0.11 and 0.12.0. All four Cargo locks keep 0.10.4 for RustCrypto `digest` 0.10,
whose separate `pos` field is written only after `compress` returns, so each `osv-scanner.toml`
ignores it.

For another low Scorecard Vulnerabilities score, download the run's `scorecard` artifact,
map every advisory ID to **all** scanned manifests, and check the `osv-scanner.toml` beside
each one. Upgrade or remove affected dependencies where possible; add a reasoned local
exception only after reviewing reachability and available fixes. Re-run Scorecard and confirm
the IDs disappear. Dismissing a GitHub alert alone does not change this scan.

The 2026-10-24 [Scorecard run](https://github.com/abbat/dyapp/actions/runs/40079935683)
scored 7.0 with four findings: `RUSTSEC-2026-0387` is the
`crypto-bigint 0.5.5` timing vulnerability pulled in by OpenMLS's
`p256 0.13`/`elliptic-curve 0.13` dependencies. The other three
(`RUSTSEC-2026-0370`, `-0371`, `-0372`) mark the separate `zvariant`,
`zbus_names` and `zvariant_derive` crates as unmaintained; the Linux client's
`secret-service 5.2` depends on `zbus 5`, which still uses them. A fixed
`crypto-bigint` requires the `0.7.6` series, while the current OpenMLS crypto
and credential crates still require `p256 0.13`. The maintainer accepted a
temporary, advisory-specific exception for `RUSTSEC-2026-0387` on 2026-10-24;
[issue #25](https://github.com/abbat/dyapp/issues/25) schedules its review by
2026-11-24. The [OpenMLS curve migration investigation](openmls-curve-migration.md)
records the dependency paths, rejected alternatives and compatibility gates.
A fresh CI/Scorecard run must confirm the exception is applied; its earlier red
result is not evidence of the new configuration's status.
The [Linux Secret Service migration analysis](../design/linux-secret-service-migration.md)
compares replacements for the separate zbus 5 advisory chain. The maintainer accepted
`dbus-secret-service`, but the code migration and audit are still pending; no exception
has been applied.

**Best Practices badge.** The project's entry on [bestpractices.dev](https://www.bestpractices.dev)
is registered and currently at InProgress level. The maintainer keeps its answers current at
every release; passing level remains a goal.

## Verification checklists

Practice C2, planned: the control catalogue exists, statuses and evidence do not yet.
[`controls.csv`](controls.csv) lists every control the checklist must give a status, one row
per control: `id`, `scope` (`dyappd` or `apps`), `level`, `title`, then `status` and `evidence`,
which the agent fills and a quorum reviews. Ids are the upstream ones with the revision in the
ASVS prefix, and they are never renumbered; a new revision is a new catalogue.

| Scope | Standard | Pinned source | Rows |
|-------|----------|---------------|------|
| `dyappd` | [OWASP ASVS 5.0.0](https://github.com/OWASP/ASVS/releases/tag/v5.0.0_release), levels 1 and 2 | `OWASP_Application_Security_Verification_Standard_5.0.0_en.flat.json` | 64 |
| `apps` | [OWASP MASVS 2.1.0](https://github.com/OWASP/masvs/releases/tag/v2.1.0), all controls | `OWASP_MASVS.yaml` | 24 |

Release asset sha256: ASVS `8201b20eec2908c3380ac600c91c8ba746346fbb808859366abb232027532311`,
MASVS `1175fe92ddb846c327b48f8efbd3fcb0148397cfb1ae0bb5a06f8c169e5e34a8`. ASVS rows sort by id;
the title column is the ASVS section name or the MASVS control statement.

C2 was written against ASVS 4 chapters; in ASVS 5 they map by meaning, not number:

| C2 scope | ASVS 4.0.3 | ASVS 5.0.0 |
|----------|-----------|-----------|
| input | V5 Validation, Sanitization and Encoding | V1 Encoding and Sanitization, V2 Validation and Business Logic |
| communications | V9 Communication | V12 Secure Communication |
| configuration | V14 Configuration | V13 Configuration |

ASVS 5's V5, V9 and V14 (file handling, tokens, data protection) are other topics and are not in
scope. Two sections join beyond C2: V4.2 HTTP message structure validation, because the [metrics
listener](../architecture/bootstrap.md#metrics-listener) parses HTTP itself, and V15.2 dependencies,
which the vulnerability checks below already cover. MASVS 2 has no levels: the level is the
[MASTG](https://github.com/OWASP/mastg) testing profile, pinned to MASTG 2.0.0 profiles L2 and P
(privacy). The RESILIENCE controls are tested under profile R, which is not chosen, so their status
is `out-of-profile`: the code is open, and no secret or protocol guarantee may depend on an
untampered client. A status is reviewed per platform (Android, iOS) once the apps exist; the
completeness check is planned.

## Vulnerability disclosure

Practice C8. [`.github/SECURITY.md`](../../.github/SECURITY.md) asks reporters to use GitHub's
private vulnerability reporting, which is enabled for the repository. A report becomes a
repository security advisory; a vulnerable dependency becomes a Dependabot alert.

**Fix-time SLO.** An alert or advisory is open from its creation until it is fixed, dismissed,
closed or published: at most 48 hours when critical, 7 days when high. Medium and low ones follow
the normal flow. `tests/scripts/vulnerability-slo.py` measures it locally (the CI token cannot read
Dependabot alerts); it needs a `gh` token with security-events access:

```bash
python3 -B tests/scripts/vulnerability-slo.py   # JSON summary; exit 1 lists each breach
```

It also fails when a published or closed repository advisory has no test that names its GHSA or
CVE id (`git grep` over `tests/`, `rust/*/tests`, `rust/*/src` and `tests/fuzz/`). Run it with the
weekly [flow report](../development/flow-metrics.md). Every alert and advisory is also an A8
trigger: the flow report expects a [postmortem](../development/flow-metrics.md#postmortems-a8)
bead within 48 hours, and an SLO breach belongs in it. The fix itself follows the normal flow.
