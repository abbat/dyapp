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
cannot remove the advisory. Do not silence it in `deny.toml` or an OSV exception.

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
2. Add a durable current-version sealed-state fixture and cross-provider tests for key,
   group, KeyPackage, message, commit and Welcome continuity.
3. Propose the dependency/provider decision in an ADR for human confirmation, as required
   by [the ADR process](../decisions/README.md). Only then switch implementation, update
   [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md), and run MLS tests,
   `make quality`, `cargo-audit`, `cargo-deny`, both lockfile scans and main CI.

The upstream release manifests are the immediate blocker to a version-only update. This
investigation has **not** established that libcrux plus a custom signer will pass every audit
or preserve the stored state; the checkpoints above decide that experimentally.
