# 0024. Historical TUF proposal for unattended bootstrap updates

- **Status:** Rejected for v0.0.1 by [ADR 0023](0023-bootstrap-node-update-policy.md)
- **Date:** 2026-10-31

## Context

The former ADR 0023 required an opt-in unattended bootstrap installer with
approval independent of the GitHub release workflow. This ADR selected TUF
metadata and offline signing as that installer's approval channel. A
compromised workflow can build and attest a malicious package; provenance alone
cannot authorize unattended installation.

ADR 0023 now keeps installation under explicit operator control for v0.0.1.
The project has no unattended installer to consume TUF metadata.

## Decision

Do not publish TUF metadata or require offline TUF signing for the v0.0.1
bootstrap release. The prior TUF role separation, rollback, expiry and
key-custody proposal remains in git history as research if unattended
installation is reconsidered. It is not a current release gate.

GitHub release artifacts, checksums and provenance remain evidence for the
[manual upgrade procedure](../manuals/bootstrap.md#updates), not authorization
for automatic execution. The separate
[client update feed](../design/client-updates.md) is unaffected.

## Consequences

The release process needs no TUF keys or metadata publisher. It also has no
independent machine-verifiable approval channel for automatic installation, so
automatic installation must remain absent. Operators still verify package
origin, digest and provenance before a manual upgrade. A future unattended
policy needs a new ADR addressing signer custody, installer privilege, backup
and coordinated rollout.
