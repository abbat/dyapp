# 0024. TUF channel for unattended bootstrap updates

- **Status:** Accepted
- **Date:** 2026-10-23

## Context

[ADR 0023](0023-bootstrap-node-update-policy.md) requires an authenticated
release channel whose approval root is independent of the GitHub release
workflow. A compromised workflow can build and attest a malicious package;
provenance alone cannot authorize unattended installation. Debian packages
are currently published through GitHub Releases, with no APT repository.
The [channel analysis](../design/bootstrap-update-channel.md) compares TUF,
signed APT and a custom signed manifest.

## Decision

Use The Update Framework (TUF) for bootstrap update metadata. A node operator
opts in by installing a pinned root trust file with the separate updater.
The updater accepts an exact `dyappd_<version>_amd64.deb` only after verifying
TUF roles, metadata versions and expiry, target path, SHA-256 and size, stable
tag and source commit. It also verifies the GitHub build attestation against
the pinned repository, workflow and commit. CI provenance is supporting
evidence; the TUF target signature is the independent installation approval.

One offline release signer approves each target digest after reviewing the
artifact and provenance. Its private targets key is absent from GitHub Actions,
GitHub secrets, the node host and the repository. The root key is used only
offline for role/key changes. A separate offline reserve root key, held apart
from the active signing setup, provides recovery if the active root key is lost.
Root metadata authorizes both root keys with a one-key threshold for rotation;
that is an accepted single-key compromise risk. Neither key is automatically
invoked by CI.
Online snapshot/timestamp keys belong to a restricted metadata publisher
outside the release workflow and cannot authorize a new target on their own.
Exact storage medium, custodian and verifier library are implementation choices
to document before generating production keys.

The updater persists trusted metadata and its highest authorized and installed
versions atomically. It rejects unsigned, stale, expired, replaced, replayed,
same-tag-different-digest and downgraded releases. An invalid clock or lost
metadata service stops automatic installation. Key rotation must follow TUF's
old/new root signature procedure and be rehearsed, including a lost active
key. A bad installed version is repaired by a newly signed higher version or
a manual operator restore; no unattended downgrade exception is allowed.

The signer and key custodian are a single-person availability and insider-risk
boundary, chosen for frequent early releases. Keep an encrypted recovery copy
and document access and succession without giving CI access. This ADR does not
enable installation: the backup, privilege, rollout and independent-review
gates in ADR 0023 still apply.

## Consequences and alternatives

TUF supplies defined role separation, rotation, expiry and rollback rules
instead of a project-specific signature protocol. It requires an offline
approval step for each release, a metadata publisher and a verified client.
A compromised offline signer can approve malware; the reserve root key can
recover trust only if it remains independent and uncompromised. A lost signer
can delay updates. Signed APT metadata would use familiar package verification
but still need independent key custody and separate rollout/version controls.
A one-file signed manifest would be simpler to publish but would require
implementing those controls ourselves.
