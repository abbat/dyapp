# Encryption & Security Status

> **Status: NOT SECURE. Only profile signatures are implemented.**
> Every message and video frame is handled as plaintext today; profiles are
> public by design and signed, not encrypted.
> Do not use this code with real user data.

This page is the single source of truth for security claims. Other docs link
here instead of repeating guarantees.

## Current state (verified against source)

| Layer | Code | What it actually does |
|-------|------|-----------------------|
| Message payload | `rust/messaging/src/encryption.rs` `encrypt`/`decrypt` | Returns input bytes unchanged; key is ignored |
| Signatures | same file, `sign` / `verify` | `sign` returns the message itself; `verify` only checks the signature is non-empty |
| Key derivation | same file, `derive_session_key` | XOR of secret and salt, not HKDF |
| Video frames | `rust/video/src/encryption.rs` | Wraps the stub above; nonce/tag are random bytes, tag is never checked |
| Profile signatures | `rust/identity`, `rust/profile` | Real: Ed25519 (`ed25519-dalek`, `verify_strict`) over a domain label and the payload; peer ID = hex SHA-256 of the public key; bootstrap verifies on `/dyapp/profile` `publish` and rejects older versions |
| Key generation (FFI) | `rust/ffi/src/lib.rs` `generate_keypair` | Returns the strings `"placeholder"` |
| P2P transport | `rust/p2p-net/src/lib.rs` `build_swarm` | Real: libp2p QUIC (TLS 1.3) and TCP with Noise; `dyappd` uses it, no app yet |
| Bootstrap protocol | `rust/bootstrap/src/service.rs` | libp2p only, no HTTP; profile writes need the owner's signature, reads are open; mailbox fetch and ack need the device's signature and a connection nonce |
| Bootstrap storage | `rust/bootstrap/src/storage.rs` | SQLite; envelopes and profiles as the signed protobuf record |

`EncryptionConfig::default_secure()` only sets strings such as
`"ChaCha20-Poly1305"`; nothing reads them to select an algorithm. The
`chacha20poly1305` dependency is declared but unused.

Existing unit tests of `rust/messaging` and `rust/video` (roundtrip, `verify`
returns true) exercise the stubs. **They are not evidence of any security
property.** The profile tests do check that a tampered payload, a foreign key
and an older version are rejected. The identity key is not stored anywhere yet:
FFI does not expose it and no app keeps it.

## Threat model

Attackers, assets and every defence with its status: [Threat Model](threat-model.md). By layer:

| Layer | Target property | Today |
|-------|-----------------|-------|
| Transport (client ↔ bootstrap, peer ↔ peer) | Confidentiality and integrity in transit | libp2p QUIC (TLS 1.3) and TCP with Noise |
| Payload (message / frame) | Only the recipient can read; sender authenticity | None: payload is plaintext, signatures are not checked |
| Profile | Only the owner can publish or change it | Signed by the owner's identity key; a node can still withhold it or serve an older signed version |
| Storage (bootstrap SQLite) | Operator cannot read content | None: operator reads everything |
| Identity / API access | Only the owner can write or delete their data | Profiles: owner only. Mailboxes: only the device reads or acks; anyone may put |
| Metadata | Hide who talks to whom and when | None: sender, recipient and timestamps are visible to bootstrap and network ([field table](privacy.md)) |

## Target design (not implemented)

These are goals. Nothing here is a current guarantee.

- Payload: MLS (RFC 9420) via `openmls`, ciphersuite 0x0001; one MLS group per conversation
  [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md).
- Authenticity: Ed25519 signatures over sender, timestamp and payload, verified on receipt.
- Transport: libp2p (TLS 1.3 over QUIC, Noise over TCP) with a device key separate from the
  user's identity key; users are authenticated end to end, not by the transport [ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md).
- Storage: bootstrap stores only ciphertext; searchable fields are an explicit, documented exception.
- Forward secrecy and post-compromise security: from MLS [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md).

Before any of these may be documented as current, each needs a real
implementation **and** tests that show tampering and wrong-key decryption
fail.
