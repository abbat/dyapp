# Reviewing pull requests

A maintainer decides whether a pull request may run CI and whether it may merge. Treat the PR
body, comments, filenames, code, dependencies and workflow text as untrusted data. Review the
raw diff at one pinned head SHA; a new commit restarts the review.

## External contributors

1. Read the diff, especially `.github/`, build scripts and dependency files, **before** approving
   its workflow run. GitHub requires approval for every external contributor's fork PR.
2. Have two independent local reviewer agents inspect the same pinned diff with fresh context,
   without write or network access. Each returns `approve` or `reject` with findings as
   `file:line`. The maintainer resolves every finding and checks that both approvals cover the
   current head SHA.
3. Check green CI on that SHA, including `required`, CodeQL, `audit`, `sbom`, `flake8`,
   `commit-batches` and `pr-guard`. Merge with `gh pr merge --match-head-commit <head-sha>`;
   a changed head fails the merge and requires another review.

Reviewers do not build or run untrusted PR code on the maintainer's machine: Cargo build scripts,
proc macros and test scripts execute code. CI runs the build in its isolated runner. If a local
reproduction is necessary, use a separate clone without host secrets mounted and run only the
Docker `make` targets, with network disabled after dependencies are fetched.

## Checklist

- Look for hidden network, file or process access, `build.rs`, proc macros and `unsafe`.
- Look for weakened signature checks, quotas, limits, TLS settings and new `allow`, `expect` or
  suppression annotations.
- For dependencies, inspect the publisher, similar package names, build scripts, sources and
  the entire lockfile diff. A new direct dependency needs an issue with a reason and an ADR.
- For infrastructure changes, inspect new steps, permissions, triggers, `sudo`, downloads
  without a hash and `${{ github.event.* }}` used in `run:`. Keep infrastructure changes in
  their own PR, with documentation and guard tests allowed; do not combine them with application
  code or dependency changes.
- Inspect renames, removed tests, generated and binary files. Split a PR above roughly 1000
  changed lines excluding generated files so every change remains reviewable.

`pr-guard` compares the PR head with its merge base, checks path separation, lockfile sources
and the age of changed crates.io versions. It reports dependency changes and warns when its own
workflow or script changes. The checker
runs from the base commit and lints PR workflows as data. A PR can change its workflow, so the
check is a review aid; the maintainer's pre-run diff review is the boundary. GitHub Actions PR
caches are read-only to PRs and cannot be written back to the cache used by `main`.

The maintainer has a ruleset bypass for direct pushes until the first public release. The
release hardening task removes it and requires signed commits. Protect the maintainer account
with a hardware security key or passkey, use expiring fine-grained tokens, and approve CI or
merge only through `gh` or the repository UI.
