# Security Assurance

How the project measures its own security practices and handles reported vulnerabilities.

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
  `tests/scripts/baselines.toml`. Below the target of 7.0 it lists the checks that lose points; at
  0.5 above the baseline it asks to raise the baseline. Artifact **`scorecard`** keeps the JSON.

The first run (2026-10-19, before `SECURITY.md`) scored 5.9. With `SECURITY.md`,
digest-pinned images, hash-pinned flake8 and the OSV exceptions it rose to 6.8.
The 2026-10-20 run on `fd8c98b` measured 7.3. CI-Tests scores only merged pull requests among
the recent commits; once the last one left that window the check became inconclusive and dropped
out of the average. After the local `tests/docker/Dockerfile.linux-test` was added, Scorecard
counted one more unpinned local base image and the score fell from 7.2 to 7.1.
The 2026-10-23 run scored 6.5: the Vulnerabilities check found 13 OSV advisories and scored 0.
The 7.1 baseline and 7.0 target are currently unmet; the scan-source mismatch below explains
why earlier reviews and root-level exceptions did not clear these findings.
Maintained remains at 0 while the repository is younger than 90 days; Code-Review and
Contributors are also at 0. The registered Best Practices badge scores 2 and branch
protection scores 4. Improving these checks requires maintainer decisions.

**Exceptions.** Scorecard's Vulnerabilities check runs OSV-Scanner over supported manifests.
OSV-Scanner reads `osv-scanner.toml` from the scanned manifest's directory; a root exception
therefore does not cover a child directory. Each reviewed exception needs a reason tied to its
scan source (no fixed version reachable, a code path the project never takes). cargo-deny keeps
its own list in `deny.toml`.
The `glib` 0.18.5 alert is accepted on Linux until Tauri's GTK 3 stack can use the fixed 0.20
series: the affected `VariantStrIter` API is not used by the app. The Windows alert was dismissed
because this Linux dependency is not compiled for Windows.
Pinned-Dependencies cannot pin the locally built `dyapp:*` base images or a stage of the same
Dockerfile; those findings are dismissed in code scanning as false positives, and so is the
Gradle wrapper JAR under Binary-Artifacts (the checked-in Gradle bootstrap).

**2026-10-23 Scorecard investigation.** The 13 Vulnerabilities findings remained after
reviewing the GitHub alerts because Scorecard runs its own OSV scan. Four Rust advisories
(`RUSTSEC-2026-0374`, `RUSTSEC-2026-0330`, `RUSTSEC-2026-0331`, `RUSTSEC-2026-0173`)
occur in both the root and `tests/fuzz/Cargo.lock`; the root `osv-scanner.toml` covers them,
but `tests/fuzz/osv-scanner.toml` does not. The other nine GHSA IDs match Maven artifacts
listed in `clients/android/gradle/verification-metadata.xml`; that directory has no
`osv-scanner.toml`. This is a manifest-to-exception coverage gap, not evidence that the
prior reviews were undone. [OSV-Scanner configuration](https://github.com/google/osv-scanner/blob/main/docs/configuration.md)
scopes exceptions to a scan source's directory; [supported manifests](https://github.com/google/osv-scanner/blob/main/docs/supported_languages_and_lockfiles.md)
include Cargo locks and Gradle verification metadata.

For another low Scorecard Vulnerabilities score, download the run's `scorecard` artifact,
map every advisory ID to **all** scanned manifests, and check the `osv-scanner.toml` beside
each one. Upgrade or remove affected dependencies where possible; add a reasoned local
exception only after reviewing reachability and available fixes. Re-run Scorecard and confirm
the IDs disappear. Dismissing a GitHub alert alone does not change this scan.

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
| `dyappd` | [OWASP ASVS 5.0.0](https://github.com/OWASP/ASVS/releases/tag/v5.0.0_release), levels 1 and 2 | `OWASP_Application_Security_Verification_Standard_5.0.0_en.flat.json` | 60 |
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
scope. MASVS 2 has no levels: the level is the [MASTG](https://github.com/OWASP/mastg) testing
profile, pinned to MASTG 2.0.0 profiles L2 and P (privacy). The RESILIENCE controls are tested
mainly under profile R, so their status records whether L2 or P tests apply. A status is
reviewed per platform (Android, iOS) once the apps exist; the completeness check is planned.

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

