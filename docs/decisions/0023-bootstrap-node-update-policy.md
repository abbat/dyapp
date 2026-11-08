# 0023. Manual bootstrap node upgrades for v0.0.1

- **Status:** Accepted
- **Date:** 2026-10-31

## Context

A node upgrade restarts service, can migrate SQLite stores in place, and resets
peer trust for `network.storage_trust_minutes`. The former policy required an
opt-in unattended installer before v0.0.1, with an independent release channel,
a privileged installer, a proven stopped-node backup, staged rollout and failure
rehearsal. Those gates have not been delivered. Manual rolling upgrades
preserve operator control and the existing node privilege boundary.

## Decision

For v0.0.1, upgrading `dyappd` is an **explicit operator action**. The
project publishes packages and provenance, but ships no unattended bootstrap
installer, updater CLI or configuration, service, timer or automatic package
channel. The network node has no package-installation privilege.

Operators follow the [manual procedure](../manuals/bootstrap.md#updates):
verify the exact release artifact and provenance; stop the node; take and prove
a restorable snapshot including a portable identity-key recovery path; upgrade
one node at a time; check reads and writes; and wait through the storage trust
window before advancing. A single-node operator plans a maintenance
interruption. An old binary must not run blindly on a newer store format.
Rollback across migration requires a compatible stopped-node snapshot and may
discard writes made after that snapshot.

The separate [client update feed](../design/client-updates.md) announces
per-platform client releases and never installs one. It is unaffected by this
node policy.

## Consequences

Manual upgrades preserve the node privilege boundary and avoid an independent
signer, privileged installer and automatic rollout coordinator for v0.0.1.
Operators may receive critical fixes later if they miss or defer a release;
release notes and the manual must make verification, backup, canary order and
recovery practical. The former unattended policy remains in git history.
Any future automatic installation requires a new explicit ADR and reviewed
trust, privilege, backup and rollout design.
