# 0023. Opt-in unattended bootstrap updates for v0.0.1

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

`dyappd` currently publishes GitHub `.deb`/tarball and GHCR artifacts, but has
no updater or APT repository. A restart resets peer trust for
`network.storage_trust_minutes`; a new binary can migrate SQLite stores in
place, after which the old binary needs a stopped-node backup to run again.
The [threat analysis](../design/bootstrap-update-policy.md) compares manual,
notification-only, package-managed and unattended updates. It recommended
manual updates because the repository has no trusted automatic package channel,
proven rollback or node coordination yet. The maintainer chose automatic
installation before v0.0.1 to shorten delivery of frequent, important updates
and accepted the added implementation and security work.

## Decision

Ship an **operator opt-in** unattended bootstrap updater before v0.0.1. It is
off until that node's operator enables it; the node process itself never gains
package-installation privileges. The first release must not claim or enable
automatic installation until all of these gates have passed:

1. An immutable, authenticated release channel pins the repository, tag,
   artifact digest and publisher identity; the updater rejects unsigned,
   replaced, stale and downgraded releases. Its signing or approval trust root
   is independent of the release workflow, so compromise of that workflow
   alone cannot authorize an install. The channel and key custody get a
   separate design and adversarial review.
2. A separate least-privilege installer verifies the exact artifact and cannot
   accept installation commands from peer traffic or the release feed. Discovery
   does not gain access to node identity keys or write access to the package.
3. Before any restart or migration, the updater verifies a restorable stopped-
   node data snapshot and an encrypted identity-key recovery path. It has a
   tested failure state for newer store formats; it must not blindly run an old
   binary on migrated data or erase later writes.
4. An operator-defined rollout limits concurrent restarts, verifies read/write
   canaries and waits the storage trust window before advancing. A single-node
   installation explicitly reports its unavoidable maintenance interruption.
   Operators can pause and recover an update.
5. Multi-node and failure-injection rehearsals cover compromised or stale
   metadata, partial download, disk full, power loss, failed migration, failed
   health check and loss of the publisher. An independent review checks the
   privilege and recovery boundaries before release.

The existing [manual procedure](../manuals/bootstrap.md#updates) remains the
only supported update path while these gates are unfinished. Failure to meet
them blocks this v0.0.1 requirement; changing the release policy requires a
new explicit maintainer decision. Implementation is tracked separately from
this policy ADR.

## Consequences

**Benefit:** opted-in operators can receive critical fixes sooner without
manually installing each version. **Cost:** the project must maintain a trusted
publisher, privileged installer, backup/recovery path and rollout coordination;
a compromised publisher or broken migration remains a serious host and data
risk. The gates reduce those risks but cannot prove an update safe. Operators
who do not opt in continue to use the manual procedure. Manual-only and
notification-only options have less implementation and privilege risk but slower
patch delivery; the [analysis](../design/bootstrap-update-policy.md#options-under-the-same-criteria)
records their pros and cons.
