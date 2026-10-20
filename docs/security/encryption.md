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
`"ChaCha20-Poly1305"`; nothing reads them to select an algorithm, and no
cipher crate is a dependency.

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
- Video frames: SFrame ([RFC 9605](https://www.rfc-editor.org/rfc/rfc9605)), a key per sender
  from the MLS exporter, a 64-bit counter as the nonce, never a random nonce.
- Attachments: a fresh random 256-bit key per attachment, AES-256-GCM per chunk with the chunk
  index as the nonce, key and blob hashes inside the MLS message; the release secret is 32 random
  bytes.
- The public device directory is an identity-signed field of the versioned profile; each
  identity-signed certificate binds a device mailbox key to a distinct MLS signature key.
  Clients keep the highest profile version seen, query at least two replica points, and use
  KeyPackages only for devices in that directory. Packages expire within 30 days; ordinary
  packages are allocated once per replica slot where possible, with a reusable last-resort
  package for exhaustion ([ADR 0019](../decisions/0019-device-directory-and-keypackage-rendezvous.md)).
- Only ciphersuite 0x0001 is accepted; a KeyPackage or Welcome with any other is rejected.
  Nothing is negotiated: a new algorithm comes with a new domain label or ciphersuite and a dated
  cut-over. 0x0001 is not post-quantum; a hybrid suite is planned once openmls ships an audited
  one.

Before any of these may be documented as current, each needs a real
implementation **and** tests that show tampering and wrong-key decryption
fail.

## Keys

One key per purpose; no key signs or encrypts for another.

| Key | Signs or encrypts | Stored | Status |
|-----|-------------------|--------|--------|
| Identity (Ed25519) | Profile, device list | Client, wrapped by the platform keystore | Generated in `rust/identity`; storage planned |
| Device (Ed25519) | Mailbox fetch and ack, media keep and attach; its hash is the mailbox address | Client, wrapped like the identity key | Planned |
| Envelope (Ed25519) | One envelope (sealed sender): the node sees no published key | Not stored, fresh per envelope | Planned |
| MLS signature (Ed25519) | MLS leaf credentials only, separate from the device key | Client, with the MLS state | Planned |
| libp2p (Ed25519) | TLS and Noise handshakes; on a node also deny lists, under their own label | Client: per device. Node: a `systemd-creds` credential sealed to the host and its vTPM, or a 0600 key file | Node: implemented. Client: planned |
| MLS group and SFrame keys | Messages, frames | Derived by MLS, never stored outside its state | Planned |
| Attachment key | One attachment | Inside the MLS message | Planned |

Rules:

- Randomness for keys, nonces and secrets comes only from the OS generator (`getrandom`). It
  blocks until the kernel pool is seeded and fails closed.
- Every Ed25519 check goes through `dyapp_identity::SignedRecord::verify` (`verify_strict`). Each
  record kind has its own label `dyapp/<kind>/v1\0`. One registry generates every domain,
  label and test case. Tests require distinct labels with exactly one trailing NUL, pairwise
  prefix freedom, separation from the locked libp2p peer-record, Noise, TLS and pubsub signing
  contexts, and rejection when a signature is verified under any other domain. The context
  snapshots are rechecked when those dependencies change.
- A secret key crosses FFI only into the platform keystore's wrap and unwrap calls, never as a
  string, and copies are wiped after use (planned).
- `dyappd` reads the node key from `storage.key`, from the unit's `node.key` credential or from
  `storage.dir/node.key`, and leaves its protection to the host; the package default is a
  credential sealed by `systemd-creds` ([node key](../manuals/bootstrap.md#node-key)). It
  checks an opened key file (not a credential, which systemd guards) and refuses any group/other
  permission bits, with a `chmod 600` hint; it leaves permissions unchanged. Mode 0600 is created by keygen, and 0400
  also loads. Back it up with `umask 077` ([backups](../manuals/bootstrap.md#backups)).
- The Rust identity generator guards its random buffer with `Zeroizing`; `Identity::secret`
  returns `Zeroizing<[u8; 32]>`, populated from borrowed signing-key bytes and cleared on drop.
  Callers borrow it for use; copying into unguarded arrays or strings creates separate copies.
- Owned node-key serialization and loading buffers use `Zeroizing` and are cleared on drop.
  Loading uses one fixed allocation rather than a growing buffer. This covers those owned
  buffers, not every compiler, upstream-library or OS copy.
- MLS state at rest relies on platform data protection (iOS file protection, Android
  credential-encrypted storage) and desktop disk encryption; flash may keep deleted epoch
  secrets.
