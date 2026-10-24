# OpenMLS curve dependency migration

Status: investigation, 2026-10-24. No dependency or MLS state format has changed.

## Why the audit fails

Both `Cargo.lock` and `tests/fuzz/Cargo.lock` resolve `crypto-bigint 0.5.5` through
`elliptic-curve 0.13.8`. [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html)
reports a timing vulnerability and requires `crypto-bigint >=0.7.6`. The root lockfile has
three paths to the old `elliptic-curve`:

| Dependency in `rust/messaging` | Unused curve dependency that keeps the vulnerable version |
|---|---|
| `openmls_basic_credential 0.6.0` | `p256 0.13.2`, `p384 0.13.1` |
| `openmls_rust_crypto 0.6.0` | `p256 0.13.2`, `p384 0.13.1` |
| `openmls_rust_crypto 0.6.0` via `hpke-rs-rust-crypto 0.7.0` | `k256 0.13.4`, `p256 0.13.2`, `p384 0.13.1` |

The app accepts only MLS suite `0x0001` (X25519/Ed25519), but Cargo resolves all the
unconditional curve dependencies. The published
[`openmls_rust_crypto 0.6.0` manifest](https://docs.rs/crate/openmls_rust_crypto/0.6.0/source/Cargo.toml)
and [`openmls_basic_credential 0.6.0` dependencies](https://docs.rs/crate/openmls_basic_credential/0.6.0)
still require `p256 0.13`. Merely updating the lockfile or replacing only one of these crates
cannot remove the advisory.

## Temporary risk acceptance

On 2026-10-24 the maintainer accepted the residual risk until a review by
**2026-11-24** ([tracking issue #25](https://github.com/abbat/dyapp/issues/25)).
The application permits only MLS suite `0x0001` (X25519/Ed25519), whereas the
vulnerable crate is retained by P-curve dependencies. That limits the intended
runtime path; it does **not** prove that vulnerable operations are unreachable.
The exception names only `RUSTSEC-2026-0387` in `deny.toml`, `.cargo/audit.toml`,
and the OSV configurations beside the root and fuzz Cargo locks. Other advisories,
thresholds and duplicate-version checks remain in force. Remove all four entries
when a compatible replacement passes the migration gates below; if still blocked
at the review date, record the upstream reason and set a new date.

## Candidate path and gates

OpenMLS documents a [libcrux provider](https://github.com/openmls/openmls) and permits a
consumer-provided crypto provider. Investigate its current release as a replacement for
`openmls_rust_crypto`. Replace `openmls_basic_credential::SignatureKeyPair` with a small
Ed25519-only `Signer` implementation using a maintained signing crate. This is a candidate,
not an accepted architecture decision: first resolve the full dependency tree and confirm that
the provider supports suite `0x0001` on every target. A provider swap alone does not remove
the basic-credential crate's `p256` dependency.

`Device::seal` stores the certificate followed by every OpenMLS storage key/value; `Device::open`
restores those bytes and reads the existing signature key by its certified public key. Before
removing the old crates, capture a sealed-state fixture with the current implementation. The
replacement must open that fixture, recover the **same** Ed25519 private key, read the stored
group and unused KeyPackage, then exchange a message and process a commit/Welcome with a
current-provider peer. If the new provider's storage layout differs, design an explicit
versioned state migration and test it before switching. Never infer compatibility from an
unchanged MLS wire ciphersuite alone.

Implementation checkpoints:

1. In an isolated change, resolve the candidate provider and signer dependencies for the
   root and fuzz workspaces. Inspect `cargo tree -i crypto-bigint@0.5.5` and both locks;
   reject a candidate that retains the vulnerable crate or introduces another failing audit.
2. Use the checked-in OpenMLS 0.9.1/RustCrypto 0.6.0 sealed-state fixture and add
   cross-provider tests for key, group, KeyPackage, message, commit and Welcome continuity.
3. Propose the dependency/provider decision in an ADR for human confirmation, as required
   by [the ADR process](../decisions/README.md). Only then switch implementation, update
   [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md), and run MLS tests,
   `make quality`, `cargo-audit`, `cargo-deny`, both lockfile scans and main CI.

The upstream release manifests are the immediate blocker to a version-only update. This
investigation has **not** established that libcrux plus a custom signer will pass every audit
or preserve the stored state; the checkpoints above decide that experimentally.

### Candidate source check (2026-10-24)

The published [`openmls_libcrux_crypto 0.4.0` manifest](https://docs.rs/crate/openmls_libcrux_crypto/0.4.0/source/Cargo.toml)
uses `hpke-rs-libcrux 0.7` and the same `openmls_memory_storage 0.6` API as the
current provider. Its [crypto implementation](https://docs.rs/openmls_libcrux_crypto/latest/src/openmls_libcrux_crypto/crypto.rs.html)
lists suite `0x0001` among the supported ciphersuites. The
[`hpke-rs-libcrux 0.7` feature list](https://docs.rs/crate/hpke-rs-libcrux/0.7.0/features)
shows its RustCrypto P-curves as optional, with no default features.
These are reasons to **test** this provider; they do not establish the final resolved
dependency tree or saved-state compatibility. A fresh advisory database found
[RUSTSEC-2026-0329](https://rustsec.org/advisories/RUSTSEC-2026-0329.html) in its
`libcrux-hmac-drbg 0.0.1` dependency. The published provider's `0.0.1` requirement
excludes fixed `0.0.2`, so the published 0.4.0 release cannot pass the current
security gate. Upstream has bumped this requirement in its source, but pinning
that Git revision alongside registry OpenMLS 0.9.1 yields two incompatible
`openmls_traits` crate instances.

A prototype copied the published provider 0.4.0 source and changed its manifest
to use the fixed DRBG and a coherent newer libcrux/HPKE set. The old-provider MLS
fixture passed, including reading sealed state, exchanging messages and applying a
commit. Fresh `cargo-deny` advisories passed, but its strict duplicate-version ban
still failed: `libcrux-intrinsics 0.0.9` depends on `core-models 0.0.7` (and hence
`hax-lib 0.3.7`) while also pulling `hax-lib 0.4.2`. The same applies to
`hax-lib-macros` and `hax-lib-macros-types`. No exception was added for the
prototype's new failures. The prototype is **not an accepted migration** until
upstream resolves this dependency chain or another provider passes all gates.

The [published provider source](https://docs.rs/crate/openmls_libcrux_crypto/0.4.0/source/src/lib.rs)
uses `openmls_memory_storage 0.6` and offers a fallible `Provider::new`; prefer that over its
panicking `Default` constructor.

The [old signer source](https://docs.rs/crate/openmls_basic_credential/0.6.0/source/src/lib.rs)
stores an Ed25519 key under an ID made from its public key, `RustCryptoSignatureKey` and the
signature scheme. Its stored value serializes `private`, `public` and `signature_scheme`; the
private bytes use the legacy JSON byte-array representation. A replacement signer must read
that exact key and verify that it derives the certified public key. Matching the MLS wire suite
alone does not prove stored-key compatibility.

`make security` can pass while current CI fails: the local Docker target runs
`cargo deny --frozen` with the advisory database baked into the image at build time, whereas CI saw the
new [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2026-0387.html). Treat fresh CI
`cargo audit` and `cargo deny` as the acceptance check; a local pass is only a check against its
bundled database.

The tested prototype and its lockfile changes were discarded after the audit
failure. The current `openmls_basic_credential` and `openmls_rust_crypto` direct
dependencies remain in place; both must be removed when a safe replacement is
available, or the vulnerable chain remains.

## Current-provider fixture

`rust/messaging/src/mls/fixtures/openmls-0.9-rust-crypto-0.6/` holds hex-encoded
test-only state generated on 2026-10-24 with OpenMLS 0.9.1, RustCrypto 0.6.0,
BasicCredential 0.6.0 and suite `0x0001`. It contains two sealed device states, their
group ID, and an unused KeyPackage plus its hash reference. Identity secrets (`0x11` and
`0x22`) and sealing keys (`0x41` and `0x42`) are fixed test values, never production
secrets. The MLS signer, group and KeyPackage keys were created by the old provider.

`current_provider_opens_saved_mls_state_fixture` reads the fixture on every test run.
It checks both certified signers, loads both group states, exchanges a message, and
retrieves the exact unused KeyPackage from storage. It uses the stored hash reference
because KeyPackage validation would expire 30 days after capture. The ignored
`write_current_provider_fixture` test generated the files into `/tmp/ai/mls-fixture`;
do not regenerate or replace this baseline when changing providers. The new provider
must pass this same fixture before the old implementation is removed.
