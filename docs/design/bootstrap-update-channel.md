# Bootstrap update channel and key custody: proposal

**Status:** draft for maintainer decision. No channel, signing keys or updater exist.
This narrows [ADR 0023](../decisions/0023-bootstrap-node-update-policy.md);
it does not change its release gate.

## Boundary and choice

The release workflow can publish an attested `.deb`, but its own provenance
cannot authorize unattended installation: a compromised workflow can attest its
own malicious output. The installer must verify a digest approved through a
separate trust root before it installs anything. The node process must never
hold signing or package-installation keys.

| Channel | Benefit | Cost and remaining risk |
|---|---|---|
| **Recommended: TUF metadata with offline target approval** | Established root, targets, snapshot and timestamp roles, expiry, version and key rotation rules; a release-CI compromise alone cannot approve a new digest. | A metadata publisher, key custody and an audited verifier are substantial new operations. An offline approver still needs to validate each build. |
| Signed APT repository with a key outside CI | APT already verifies signed Release metadata and package hashes; operators know its configuration. | An independent signer/repository is still required. APT alone does not coordinate node restarts, preserve stores or enforce the app's monotonic version policy. Repository-key compromise authorizes packages. |
| Pinned one-file signed manifest | Few moving parts to publish initially. | Implementing expiry, rollback, key rotation and metadata consistency recreates part of TUF; an error would authorize privileged installation. |

The proposed TUF profile uses a root trust file installed with the updater,
kept independently of GitHub. The root key stays offline with an encrypted
recovery copy under separate physical custody. A separate offline targets key
signs the exact Debian package path, SHA-256, size, tag and source commit after
human review of the build and provenance. CI has neither private key. Online
snapshot/timestamp keys may be held by a restricted publisher outside the
release workflow. The publisher cannot add a target without a targets signature.
An operator can retain manual updates if they do not opt in.

Root and targets key rotation requires an operator-reviewed transition signed
under the old trust root and the new one, with an explicit recovery procedure
for a lost key. No key material enters the repository, GitHub secrets, node
identity store or `dyappd` process. Key custody, signer count and who can use
the offline key remain maintainer decisions; a single signer is an availability
and insider-risk trade-off, while a threshold requires multiple custodians.

## Installer rejection rules

1. Fetch only the configured metadata origin over HTTPS. Bound every response,
   reject redirects to another origin, unknown roles, signatures or algorithms.
2. Verify TUF root and role signatures, metadata versions, expiry, hashes and
   lengths; persist trusted metadata atomically. Fail closed on stale, forged,
   replaced or missing metadata. Clock failure blocks installation.
3. Require the target to match the exact repository, stable tag, commit,
   `dyappd_<version>_amd64.deb` name, digest and size. Independently verify its
   GitHub build attestation against the pinned release workflow and commit.
   An attestation is supporting provenance, not approval.
4. Persist the highest authorized and installed versions; reject a lower
   version, reused tag with a new digest, and metadata rollback. Recovery from
   a bad version uses a newly approved higher release, or a manual operator
   restore with an explicit exception outside the unattended path.
5. The privileged installer accepts only a locally verified immutable package
   descriptor. It cannot take a URL, digest or command from peer traffic.
   Backup, rollout and host-isolation gates remain separate beads.

Until this design is approved, the updater work must not select a signing
library or generate production keys. After approval, add an ADR and adversarial
fixtures for forged targets, stale metadata, key rotation, replayed/changed
tag, mismatched provenance and lower package version.

## Evidence

- [TUF specification](https://theupdateframework.github.io/specification/v1.0.37/index.html)
  defines signed roles, thresholds, expiry, monotonic versions and rotations.
- [Debian apt-secure](https://manpages.debian.org/unstable/apt/apt-secure.8.en.html)
  authenticates signed Release metadata but does not protect a compromised
  signing master or key.
- [GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
  establish build provenance; GitHub says they do not guarantee safety.
