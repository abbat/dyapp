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
