# Encryption & Security Status

> **Status: NOT SECURE. Profile and KeyPackage record signatures, the MLS core and its
> delivery through device mailboxes (`rust/client`) are implemented.** The app does not use the
> delivery yet, no media path carries SFrame-encrypted frames yet and metadata is not hidden; profiles are public
> by design and signed, not encrypted.
> Do not use this code with real user data.

This page is the single source of truth for security claims. Other docs link
here instead of repeating guarantees.

## Current state (verified against source)

| Layer | Code | What it actually does |
|-------|------|-----------------------|
| Message payload | `rust/messaging/src/mls.rs` | Real in Rust: MLS (RFC 9420) via openmls 0.9, ciphersuite 0x0001 only. The identity key certifies a device's MLS signature key in a `DeviceCertificate`, which is the device's MLS basic credential; every KeyPackage, Welcome member, commit leaf and message sender must chain to a Peer ID and be listed in the owner's newest profile the client holds, so a device the owner dropped can no longer join, commit or send, and `Group::remove_revoked` removes it; another device of the owner closes its mailbox (`Node::close_mailbox`, with the `owner_binding` the device signed at linking), and the dropped device learns it was revoked from `chat::revoked` and must wipe its keys. A KeyPackage that is expired, valid for over 30 days or with another suite, and a commit that changes group extensions, are refused. `profiles::Profiles` holds the newest profile per owner and refuses a lower version, and `profiles::newest` takes the highest valid version among several replicas, so a node cannot bring a dropped device back with an older profile. `Device::seal` encrypts the whole MLS state (signature key, unused KeyPackages, groups) with ChaCha20-Poly1305 under a 32-byte key the client keeps wrapped by the platform KeyStore, and `Device::open` and `Group::load` restore it; all of it is exported over FFI as `MlsDevice`/`MlsGroup`. Storing the state and the held profiles is the app's job |
| Message delivery | `rust/client` | Real in Rust, not used by the app yet: `chat::invite` fetches a KeyPackage for every device in the members' profiles and puts the Welcome in each device mailbox; `chat::message` pads the plaintext to the next power of two from 1 KiB and puts one MLS application message per member device; `chat::receive` fetches the mailbox, joins or applies each envelope and returns the ids to ack once the state is sealed. A node stores and returns only MLS bytes. A Welcome naming a member whose owner's profile the receiver does not hold returns `Event::UnknownOwner` and stays in the mailbox with its KeyPackage kept, so it joins once the app fetches that profile and receives again; any other refused envelope is acked; write tokens are not used |
| Private interactions | `rust/client/src/interactions.rs` | The v1 26-byte like, unlike and view plaintext codec rejects malformed, extra-byte and unknown-version or kind payloads; a test carries the bytes through the existing MLS chat path. A short-padded signal sender, durable state, match calculation and app use remain planned |
| Client local store | `rust/client/src/store.rs` | A versioned SQLite schema opens only for its bound identity and rejects unknown or corrupt databases; WAL uses full synchronous commits. It persists per-profile view-send and pass choices, but no chat or MLS state writes the reserved tables yet, and no app opens the store |
| Prepared mailbox writes | `rust/client/src/lib.rs` | The client can prepare a signed ordinary MLS envelope before sending and reopen verified exact bytes with its stable mailbox and ID for retry; no durable outbox or receipt flow uses this yet |
| Chat attachments | `rust/client/src/attachment.rs` | Real in Rust, not used by the app yet: `attachment::seal` encrypts a blob under a fresh random 256-bit key, AES-256-GCM per 64 KiB chunk with the chunk index as the nonce and the last chunk marked in the additional data, and returns an `Attachment` with the key, the plaintext and blob SHA-256 and a 32-byte release secret from `getrandom`; `attachment::open` checks the blob hash, decrypts every chunk and checks the plaintext hash. Carrying the `Attachment` in the chat message and uploading the blob over `/dyapp/media` are planned |
| Video frames | `rust/video/src/sframe.rs` | Real in Rust, no media path uses it yet: SFrame (RFC 9605) with cipher suite `AES_128_GCM_SHA256_128`; the base key is the epoch's MLS exporter secret for `SFrame 1.0 Base Key`, every member encrypts under its own KID (member index and low four epoch bits), the nonce is the 64-bit frame counter XOR the salt and the sender refuses to wrap it; a receiver refuses a changed header, frame or metadata, an unknown KID, a counter it has accepted and one 64 or more below the highest. Exporting the base key and the member indices for a call is planned |
| Profile signatures | `rust/identity`, `rust/profile` | Real: Ed25519 (`ed25519-dalek`, `verify_strict`) over a domain label and the payload; peer ID = hex SHA-256 of the public key; bootstrap verifies on `/dyapp/profile` `publish` and rejects older versions. Device certificates in the profile are independently signed by that identity key and checked for unique, sorted device and MLS keys |
| KeyPackage rendezvous | `rust/bootstrap/src/service/keypackages.rs` | Node verifies the device's outer record signature and current profile certificate, bounds size and expiry, and consumes ordinary stock locally before reply. Node checks only the MLS suite header; the client checks the MLS signature, credential and lifetime when it adds the device (`Device::create_group`) |
| Identity key storage (FFI) | `rust/ffi/src/lib.rs` `IdentityKey`, `KeyStore` | Real in Rust: the secret leaves only through the app's `KeyStore` wrap and unwrap; Android wraps with a Keystore AES-256-GCM key, iOS and macOS with a Keychain one; desktop keystores are planned. App data stays out of OS backups and device transfer: Android sets `allowBackup="false"` with data extraction rules that exclude everything, iOS and macOS mark the app folder `isExcludedFromBackup` |
| P2P transport | `rust/p2p-net/src/lib.rs` `build_swarm` | Real: libp2p QUIC (TLS 1.3) and TCP with Noise; `dyappd` uses it, no app yet |
| Bootstrap protocol | `rust/bootstrap/src/service.rs` | libp2p only, no HTTP; profile writes need the owner's signature, reads are open; mailbox fetch and ack need the device's signature and a connection nonce |
| Bootstrap storage | `rust/bootstrap/src/storage.rs` | SQLite; envelopes, profiles and KeyPackage stock as signed protobuf records |

The MLS tests (`rust/messaging/src/mls/tests.rs`) check a roundtrip both ways and that a
tampered, replayed or foreign-group message, a second join with a single-use Welcome, a forged
certificate, a device missing from its owner's profile, another suite, a too-long lifetime and a
commit adding an uncertified member are rejected, that a revoked device is removed, and that the exporter gives every member the same secret. The attachment tests check that a tampered,
reordered or truncated blob and a wrong key fail, and that no two attachments share a key or
release secret. The SFrame tests (`rust/video/src/sframe/tests.rs`) check the RFC 9605 header
and `AES_128_GCM_SHA256_128` test vectors, that a changed byte, other metadata, a wrong key, a
replayed or too old counter and a retired epoch fail, and that no counter repeats under a key.
The profile tests check that
a tampered payload, a foreign key and an older version are rejected.

## Threat model

Attackers, assets and every defence with its status: [Threat Model](threat-model.md). By layer:

| Layer | Target property | Today |
|-------|-----------------|-------|
| Transport (client ↔ bootstrap, peer ↔ peer) | Confidentiality and integrity in transit | libp2p QUIC (TLS 1.3) and TCP with Noise |
| Payload (message / frame) | Only the recipient can read; sender authenticity | MLS in `rust/messaging`, delivered by `rust/client`; frames: SFrame in `rust/video`, group members only, a member can forge another's frames ([RFC 9605 §7.2](https://www.rfc-editor.org/rfc/rfc9605#section-7.2)); the app uses neither yet |
| Profile | Only the owner can publish or change it | Signed by the owner's identity key; a node can still withhold it or serve an older signed version |
| Storage (bootstrap SQLite) | Operator cannot read content | None: operator reads everything |
| Identity / API access | Only the owner can write or delete their data | Profiles: owner only. Mailboxes: only the device reads or acks; anyone may put |
| Metadata | Hide who talks to whom and when | None: sender, recipient and timestamps are visible to bootstrap and network ([field table](privacy.md)) |

## Target design

The items below describe the intended client security model. The directory and node-side
KeyPackage checks noted above are implemented; the MLS core in
`rust/messaging/src/mls.rs` is implemented, including its sealed state, and so is its
delivery through mailboxes in `rust/client`; the app runtime on top of it, the client's state
store and the media path for SFrame remain planned.

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
| Identity (Ed25519) | Profile, device list, media keep and attach | Client, wrapped by the platform keystore | Generated in `rust/identity`; wrapped over FFI through the app's `KeyStore`; Android Keystore and Apple Keychain done, desktop planned |
| Device (Ed25519) | Mailbox fetch, ack and revoke, write tokens; its hash is the mailbox address | Client, wrapped like the identity key | Planned |
| Envelope (Ed25519) | One envelope (sealed sender): the node sees no published key | Not stored, fresh per envelope | Implemented in `rust/client` (`Node::put`) |
| MLS signature (Ed25519) | MLS leaf credentials only, separate from the device key | Client, with the MLS state | Implemented in `rust/messaging`; sealed with the MLS state by `Device::seal` |
| libp2p (Ed25519) | TLS and Noise handshakes; on a node also deny lists, under their own label | Client: per device. Node: a `systemd-creds` credential sealed to the host and its vTPM, or a 0600 key file | Node: implemented. Client: planned |
| MLS group and SFrame keys | Messages, frames | Derived by MLS, never stored outside its state | MLS implemented in `rust/messaging`; SFrame key derivation in `rust/video/src/sframe.rs`, exporting it for a call planned |
| Attachment key | One attachment | Inside the MLS message | Generated and used in `rust/client/src/attachment.rs`; sending it in a chat message is planned |

Rules:

- Randomness for keys, nonces and secrets comes only from the OS generator (`getrandom`). It
  blocks until the kernel pool is seeded and fails closed.
- Every Ed25519 check goes through `dyapp_identity::SignedRecord::verify` (`verify_strict`). Each
  record kind has its own label `dyapp/<kind>/v1\0`. One registry generates every domain,
  label and test case. Tests require distinct labels with exactly one trailing NUL, pairwise
  prefix freedom, separation from the locked libp2p peer-record, Noise, TLS and pubsub signing
  contexts, and rejection when a signature is verified under any other domain. The context
  snapshots are rechecked when those dependencies change.
- A secret key crosses FFI only into the app's `KeyStore` `wrap` and out of its `unwrap`
  (`IdentityKey`), never as a string. Rust wipes its copies; the FFI buffers that carry the
  secret are freed, not wiped.
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
