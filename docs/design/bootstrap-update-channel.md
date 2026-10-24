# Bootstrap update channel and key custody: analysis

**Status:** channel and signer model accepted in
[ADR 0024](../decisions/0024-bootstrap-update-channel.md). No signing keys,
metadata publisher or updater exist. This narrows
[ADR 0023](../decisions/0023-bootstrap-node-update-policy.md) without changing
its release gate.

## Boundary and choice

The release workflow can publish an attested `.deb`, but its own provenance
cannot authorize unattended installation: a compromised workflow can attest its
own malicious output. The installer must verify a digest approved through a
separate trust root before it installs anything. The node process must never
hold signing or package-installation keys.

| Channel | Benefit | Cost and remaining risk |
|---|---|---|
| **Chosen: TUF metadata with offline target approval** | Established root, targets, snapshot and timestamp roles, expiry, version and key rotation rules; a release-CI compromise alone cannot approve a new digest. | A metadata publisher, key custody and an audited verifier are substantial new operations. An offline approver still needs to validate each build. |
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
identity store or `dyappd` process. The maintainer chose one offline signer
plus a separate reserve root key. A single signer is an availability and
insider-risk trade-off; threshold signing was considered and not chosen.

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

The approved policy is in ADR 0024. Before generating production keys, choose
and review the verifier library, custody procedure and metadata publisher.
Adversarial fixtures must cover forged targets, stale metadata, key rotation,
replayed/changed tags, mismatched provenance and lower package versions.

## Proposed implementation profile

The Rust [`tough` client](https://docs.rs/tough/latest/tough/) 0.24.0 builds
with the repository's Rust 1.99 toolchain in an isolated Docker spike. Its
publisher is **not selected**. Source inspection of [`tuftool` 0.17.0](https://github.com/awslabs/tough/tree/tuftool-v0.17.0/tuftool)
shows that `create` and `update` pass one set of signing keys to
`RepositoryEditor::sign`, which signs targets, snapshot and timestamp in one
process. Requiring the offline targets key on the online publisher would
violate ADR 0024. A full `tuftool` build attempt exhausted the 1 GiB local
`/tmp` filesystem; that is an environment limit, not a compiler result.
Keep the role separation requirement and prove a two-stage publisher with
disposable keys before choosing a production stack.

| Stack | Benefits | Costs and decision |
|---|---|---|
| `tough` client + split-role publisher | Native Rust verifier and streaming target download; the client builds on Rust 1.99. | `tough` is not listed in the [TUF conformance results](https://theupdateframework.github.io/tuf-conformance/). The `tuftool` CLI cannot directly implement the selected offline/online custody split. A separate publisher and interoperability fixtures are required. **Client candidate only.** |
| `sigstore-tuf` + separate publisher | Rust client passes the published conformance suite. | Its [target download](https://docs.rs/sigstore-tuf/latest/sigstore_tuf/client/struct.Updater.html) buffers the entire target; a Debian package would need a separately verified streaming path and publisher interoperability tests. Fallback if `tough` fails. |
| `python-tuf` + separate updater process | Reference client passes the published conformance suite; mature repository tooling. | Adds a Python runtime and a process boundary to the privileged update path. Fallback if the Rust candidates cannot meet the gate. |
| `python-tuf` metadata API for publishing | Its [metadata API](https://theupdateframework.readthedocs.io/en/stable/api/tuf.api.metadata.metadata.html) signs individual roles, so offline targets and online snapshot/timestamp stages can be separated. | Requires a small publisher program, reviewed key handling and interoperability tests with the chosen Rust client. **Next publisher candidate.** |

The repository uses TUF 1.0 with root, targets, snapshot and timestamp; one
signing key per role except root, which authorizes the active and separately
held reserve key at threshold one. No delegated targets roles initially.
Enable consistent snapshots, SHA-256 target hashes and immutable target paths.
Install a reviewed `root.json` with the updater; never trust a root fetched
from the same metadata origin as the initial trust anchor. Root rotation must
be accepted by the old and new root keys under TUF's transition rules. The
reserve root key is used only for a rehearsed recovery or planned rotation.

The offline signer reviews the release artifact, GitHub attestation and source
commit, then signs `targets.json` containing two immutable targets for each
approved release:

- `dyappd/<sha256>/dyappd_<version>_amd64.deb`, the exact package bytes;
- `dyappd/<sha256>/approval.json`, a canonical release descriptor with schema
  version, repository, stable tag, source commit, release workflow identity,
  package target path, SHA-256, size and package version.

Both paths are covered by TUF targets hashes and lengths. The descriptor binds
the application-specific fields without depending on publisher support for
TUF's optional target `custom` field. The verifier requires the descriptor's
package path, hash and length to equal the authenticated package target. It
also checks the attestation against the descriptor and rejects duplicate
version/tag bindings, aliases, unexpected paths and mismatched architecture.
Publishing a new release adds paths; it never replaces bytes at an existing
path. This layout and descriptor schema are proposed and must be exercised by
publisher/client interoperability fixtures before they become a wire format.

Only the offline targets signer can add an approved release. A restricted
publisher outside GitHub release CI signs snapshot and timestamp after
verifying the received `targets.json` signature and that every referenced
target is already uploaded. Upload immutable targets and versioned metadata
first, then publish `timestamp.json` last. Use monotonic role versions and
short timestamp expiry; renew timestamp without re-signing targets when no
release changes. Provision separate online snapshot and timestamp keys. The
publisher must not possess root or targets private keys. Record the exact
expiry periods, ownership and recovery procedure before production keys exist.

### Two-stage publisher handoff (fixture contract)

Use the low-level `tuf.api.metadata` API from the published
[`python-tuf` 7.0.1 release](https://pypi.org/project/tuf/7.0.1/) as the
first publisher candidate. Its `Metadata` object signs one role at a time;
the repository API is not yet a stable public API. Signing needs the
`securesystemslib` cryptographic backend as well as `tuf`; pin their exact
versions and package hashes only after the disposable fixture builds. The
[Go TUF low-level metadata API](https://github.com/theupdateframework/go-tuf)
is an alternative with individual role signing, but adds a second toolchain
and still needs the same custody and interoperability proof. Neither is an
accepted publisher yet.

The **offline stage** receives a reviewed immutable `.deb`, its attestation,
the previous signed targets metadata and a trusted public root. It validates
the package and provenance, rejects a reused version/tag or changed bytes at
an existing target path, increments the targets version, sets a bounded expiry,
and signs only `targets.json` with the offline targets key. Its output bundle
contains the exact signed metadata bytes and immutable target bytes. The
offline root key is needed only to establish or rotate root metadata, never
for an ordinary release. No online role private key is mounted in this stage.

The **online stage** receives that bundle and trusted public root, but only
the snapshot and timestamp private keys. It verifies the targets signature
against the root role, expiry and monotonically increasing version; compares
all target hashes and lengths against the actual bytes; and refuses path
replacement or removal from the previously published targets set. It computes
snapshot hashes and lengths from the *exact received `targets.json` bytes*,
without reserializing them, then signs snapshot and timestamp separately.
It must reject a missing, unsigned or rolled-back handoff rather than repair
it. Timestamp-only renewal reuses the already published snapshot; it has no
targets signing path.

Publish immutable target bytes and versioned targets metadata first, then the
versioned snapshot, and make `timestamp.json` visible last. A failure before
the final step leaves the old timestamp serving the old consistent repository.
The fixture must mount disjoint private-key directories into two separate
processes, assert the online stage cannot read root/targets private keys, and
load the resulting repository with `tough 0.24.0`. It must also reject a
forged signature, altered target byte, lower role version, expired handoff and
replaced existing path. These are requirements for the next implementation
checkpoint, not completed tests.

The client is unprivileged. It pins HTTPS metadata and target origins, limits
downloads and redirects, enforces metadata expiry with a trustworthy clock,
and persists TUF state across restarts. In `tough`, explicitly configure a
[persistent datastore](https://docs.rs/tough/latest/tough/struct.RepositoryLoader.html):
omitting it creates a temporary directory and loses rollback memory. Keep
expiration enforcement safe and set explicit size limits. Stream a target
only into private staging; [`read_target`](https://docs.rs/tough/latest/tough/struct.Repository.html)
exposes bytes before its final checksum is known. The installer sees a
descriptor only after the complete stream, hash, length, provenance and
application version checks succeed, followed by an atomic handoff. Persist
the highest approved and installed application versions separately from TUF
metadata. State loss blocks unattended installation until operator recovery.

### Verification gate before enabling updates

Use an offline fixture repository and a fake package installer. The gate must
exercise a valid release; forged targets/root; omitted, modified or truncated
package and descriptor; expired metadata or invalid clock; timestamp/snapshot/
targets rollback and freeze; same tag or version bound to a different digest;
wrong repository/workflow/commit/architecture in the attestation; lower app
version; redirect to another origin; interrupted target stream and state write;
and old/new root rotation plus reserve-key recovery. Reopen stack selection if
the selected versions reject a valid fixture or accept an invalid one. No
fixture may invoke the privileged installer on rejection. These tests are
planned; none is claimed by this design document.

## Evidence

- [TUF specification](https://theupdateframework.github.io/specification/v1.0.37/index.html)
  defines signed roles, thresholds, expiry, monotonic versions and rotations.
- [Debian apt-secure](https://manpages.debian.org/unstable/apt/apt-secure.8.en.html)
  authenticates signed Release metadata but does not protect a compromised
  signing master or key.
- [GitHub artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
  establish build provenance; GitHub says they do not guarantee safety.
