# 0021. AI review with a human merge

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

External contributors can submit code and workflow changes. GitHub cannot count local AI agents
as required account approvals, and a single maintainer cannot approve their own PR. Running
untrusted PR code on the maintainer's host also exposes local credentials and tools. We need a
review policy that fits this single-maintainer repository without presenting the PR guard as a
security boundary.

## Decision

The maintainer reads an external PR diff before approving its CI run. Two independent local AI
reviewers inspect a pinned head SHA as untrusted data, without write or network access; neither
runs PR code on the maintainer's host. The maintainer resolves findings, confirms green required
checks on the same SHA and merges with `gh pr merge --match-head-commit <head-sha>`. GitHub does
not require account review approvals for this process.

A branch ruleset requires PRs and checks but grants the maintainer a direct-push bypass until the
first public release. Before that release, the bypass is removed and signed commits become
required. Infrastructure changes use a separate PR from application code and dependencies.

## Consequences

A changed head invalidates the two local reviews. The maintainer must manually enforce their
independence and the pre-run workflow review. The guard and GitHub settings reduce accidental
and opportunistic bypasses, but a compromised maintainer session can use the temporary bypass.
