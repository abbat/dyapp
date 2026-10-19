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
  `scripts/check-scorecard.py` fails when the aggregate score is below `[scorecard] score` in
  `scripts/baselines.toml`. Below the target of 7.0 it lists the checks that lose points; at
  0.5 above the baseline it asks to raise the baseline. Artifact **`scorecard`** keeps the JSON.

The baseline is 0 until the first run sets it.

**Best Practices badge.** The project's entry on [bestpractices.dev](https://www.bestpractices.dev)
is registered by the maintainer (a human gate) and kept current at every release. The badge
level is passing; until the entry exists, the badge is planned.

## Vulnerability disclosure

Practice C8. [`.github/SECURITY.md`](../../.github/SECURITY.md) asks reporters to use GitHub's
private vulnerability reporting, which is enabled for the repository. A report becomes a
repository security advisory; a vulnerable dependency becomes a Dependabot alert.

**Fix-time SLO.** An alert or advisory is open from its creation until it is fixed, dismissed,
closed or published: at most 48 hours when critical, 7 days when high. Medium and low ones follow
the normal flow. `scripts/vulnerability-slo.py` measures it locally (the CI token cannot read
Dependabot alerts); it needs a `gh` token with security-events access:

```bash
python3 -B scripts/vulnerability-slo.py   # JSON summary; exit 1 lists each breach
```

It also fails when a published or closed repository advisory has no test that names its GHSA or
CVE id (`git grep` over `tests/`, `rust/*/tests`, `rust/*/src` and `fuzz/`). Run it with the
weekly [flow report](../development/flow-metrics.md). Every alert and advisory is also an A8
trigger: the flow report expects a [postmortem](../development/flow-metrics.md#postmortems-a8)
bead within 48 hours, and an SLO breach belongs in it. The fix itself follows the normal flow.

