# Bootstrap node update policy: analysis before v0.0.1

Status: analysis and recommendation; maintainer decision pending. No unattended
bootstrap updater is implemented or enabled.

## Decision to make

Should `dyappd` install a new version without its node operator approving the
particular update? The v0.0.1 gate is a policy decision, not an implementation
gate. Independent operators own their hosts, keys and data. A node update can
restart service, migrate persistent stores and change wire behavior; these
costs must be considered separately from finding out that a release exists.

**Recommendation for v0.0.1:** keep installation under explicit operator
control. Document and rehearse manual rolling upgrades. A later, separate
notify-only feature may tell operators a release exists, with a link to the
fixed GitHub Release and verification instructions; it must not stage or run a
package with root privileges. Reconsider unattended installation only after
the safety gates below have been built and tested. This recommendation needs
maintainer approval before it becomes policy.

## Evidence and causal risks

| Condition in this repository | Failure if installation is unattended |
|---|---|
| [`dyappd.service`](../../packaging/debian/dyappd.service) runs as `dyappd` with `ProtectSystem=strict` and no capabilities. | Writing `/usr/bin/dyappd` or changing packages needs a separate privileged process. Giving the network node installation privileges widens the effect of a node compromise. |
| The [operator manual](../manuals/bootstrap.md#updates) requires one-node-at-a-time restarts and a `network.storage_trust_minutes` wait; a restarted node refuses writes during that period. | Independent nodes reacting to one new tag at once can remove the remaining writable replicas; one-node deployments lose write service during their own restart. |
| The [same manual](../manuals/bootstrap.md#rollback) says new binaries migrate SQLite stores in place, older binaries reject newer formats, and rollback needs a stopped-node backup. There is no online backup. | A failed health check cannot safely trigger a blind binary rollback. Restoring a pre-upgrade snapshot can discard later writes on that node; a single node may hold the only copy. |
| [ADR 0020](../decisions/0020-dyappd-release-channels.md) publishes `.deb`/tarballs and GHCR images from a tag; no APT repository exists. | `unattended-upgrades` will not update a manually installed GitHub `.deb` by itself. An automatic installer would need a new signed repository or custom privileged downloader and policy. |
| CI adds GitHub build provenance attestations. | Verification can establish a workflow/repository/digest relationship, but a compromised release workflow or approved malicious source can still produce an attested malicious binary. |
| The node key can be a host/TPM-bound encrypted credential ([ADR 0018](../decisions/0018-node-key-source.md)). | Moving a backup to a replacement host may not restore the node identity unless the operator kept a separate encrypted key backup. Automated recovery cannot assume one exists. |

The highest-impact path is **compromised publisher or updater → privileged
installation or execution → access to node data and host**. Another is
**automatic common-time restart → no trusted write path → unavailable network**.
A third is **store migration → failed new binary → impossible old-binary start
without a backup**. TLS, checksums or provenance each cover a narrower link;
none removes all three paths. The current manual upgrade contains two barriers:
an operator verifies the artifact and watches a canary node before touching the
next one. It can still be slow when a security release is urgent.

## Options under the same criteria

| Policy | Pros | Cons and remaining risk |
|---|---|---|
| Manual rolling upgrade (recommended for v0.0.1) | Uses the documented release/backup/rehearsal path; keeps node operator control and existing service privilege boundary; supports one-node-at-a-time checks. | Operators may miss or delay a security update; needs clear release communication and usable instructions. |
| Notify only | Faster awareness with no install privilege; operator still verifies and schedules an upgrade. | Requires a trustworthy release signal and monitoring; notice alone does not ensure action. Keep it separate from client-update metadata until the server release signal is specified. |
| Operator-approved staging/install helper | Can verify and prepare an exact artifact before approval, shortening maintenance time. | Still needs a privileged helper, exact approval UX, backup/health checks and a safe failure path; a UI click can be rushed. |
| Package-manager-managed updates | APT handles repository metadata and package transactions; operators can use familiar pinning and maintenance policy. | Needs a signed APT repository not currently present; unattended APT has no awareness of replica trust windows or store migration and can restart nodes together. |
| Unattended self-update | Shortest patch delay when everything works. | A custom privileged downloader becomes an attractive attack path; common-time restarts and migration/rollback hazards remain. No proven safe deployment path exists today. |

For package-manager automation, Debian says `unattended-upgrades` installs from
configured [APT sources](https://manpages.debian.org/trixie/unattended-upgrades/unattended-upgrades.8.en.html).
For release trust, GitHub says [attestations must be verified and do not prove
an artifact is secure](https://docs.github.com/en/actions/concepts/security/artifact-attestations).
These sources support the narrower trust claims above; they do not establish
that any update mode is automatically safe.

## Conditions to reconsider unattended installation

1. Define a publisher trust policy stronger than an HTTPS URL: pin repository,
   tag, digest, attestation identity and workflow; reject rollback, replaced
   assets and unknown channels. Model a compromised GitHub account/workflow and
   choose an independent approval or signing boundary if that threat must be
   contained. Test forged and stale metadata.
2. Keep discovery unprivileged. Any installer must be a separately constrained
   operator-owned component that never reads the node identity key or network
   requests as commands, and accepts only one verified, immutable artifact.
3. Before restart, take and verify a stopped-node store backup and an encrypted
   key backup suitable for this host. Rehearse migration, a failed start and
   restoration, including the writes that may be lost. Do not promise automatic
   rollback across a store format change without a safe data plan.
4. Roll out by an operator-defined canary/maintenance schedule. Prove that
   independent or jointly managed nodes cannot all restart inside the trust
   window. Check health, canary writes and reads, then wait the trust period
   before the next node. Define an emergency pause and recovery procedure.
5. Test partial downloads, power loss, package-manager locks, disk full,
   invalid signatures, downgrade attempts, incompatible client/node protocol,
   and loss of the release service. Every failure must leave the prior binary
   running or a clear operator-recoverable state; it must never erase stores.
6. Show evidence from a multi-node rehearsal and a review of privilege,
   release-key custody and recovery by someone other than the implementer.
   Only then request a new maintainer decision. The first release does not wait
   for these implementation gates if the approved policy is manual updates.

## Decision and verification handoff

The maintainer chooses **manual only**, **manual plus notify-only**, or a
specific automatic policy with explicit accepted risks. Record the choice in
an ADR and update the [operator manual](../manuals/bootstrap.md#updates) before
v0.0.1. The proposed owner is the maintainer; no approval has been recorded.
Revisit if a signed APT repository, safe backup/rollback mechanism and rolling
coordination become real, or a critical patch delay reveals a different balance
of risk. Until then, the current manual procedure is the observable baseline.
