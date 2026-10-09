# Architecture Overview

This page separates **what exists in the repository today** from the **target
design**. Status vocabulary matches the [README status table](../../README.md) (in Russian there):
*library prototype* (compiles, has unit tests, not wired into an app),
*stub* (types/signatures exist, core logic is `TODO`), *skeleton* (project
scaffold only), *planned* (no code).

For what each party can observe, see [Privacy and Metadata Visibility](../security/privacy.md).

## Actual architecture (today)

```text
 Platform apps (skeletons; none links the Rust core yet)
 ┌──────────────┬───────────────┬──────────────┬──────────────────────────┐
 │ android/     │ ios/, macos/  │ desktop/     │ linux/, windows/          │
 │ Kotlin       │ Swift         │ Tauri shell  │ Tauri apps (excluded from │
 │ RustBridge.kt│               │              │ the Cargo workspace)      │
 │ (loadLibrary │               │              │                           │
 │  commented)  │               │              │                           │
 └──────┬───────┴───────────────┴──────────────┴──────────────────────────┘
        ┊  intended FFI (not linked)
        ▼
 rust/ffi  (uniffi, dyapp.udl)
   ├──► rust/profile    (signed profile) ──► rust/identity (Ed25519)
   ├──► rust/messaging  (ChaCha20-Poly1305)
   ├──► rust/p2p-net    (libp2p node; not called by ffi yet)
   └──► rust/video      (webrtc) ──► rust/messaging

 rust/bootstrap  (dyappd: libp2p node protocol + SQLite, separate server process)
   └──► identity, profile, p2p-net   (Cargo dependencies; tests also use messaging, video)
```

Only `dyappd` and its test client `test-peer` talk over the network today: no app calls a
node, and there is no WebRTC signaling channel.

| Component | Source | Status | What it really does |
|-----------|--------|--------|---------------------|
| Identity | `rust/identity/src/` | library | Ed25519 identity key, `SignedRecord` (protobuf) signed over a domain label, peer ID = hex SHA-256 of the public key; the key is not persisted or exposed over FFI yet |
| Profile | `rust/profile/src/` | library | Public `Profile` (protobuf), signature and content checks, highest version wins, tombstone for deletion |
| Messaging | `rust/messaging/src/` | library prototype | Message types, Lamport clock, in-memory queue, ChaCha20-Poly1305 helpers in `encryption.rs` |
| P2P networking | `rust/p2p-net/src/` | library | libp2p 0.57 swarm: QUIC and TCP+Noise+Yamux, Kademlia `/dyapp/kad`, identify, AutoNAT; request-response `/dyapp/node`, `/dyapp/profile` with `ProtoCodec` ([P2P networking](p2p-networking.md)) |
| Video | `rust/video/src/` | library prototype | `session.rs` creates a real WebRTC offer and applies the remote answer and candidates; no callee path or media; frame encryption in `encryption.rs` |
| FFI | `rust/ffi/src/lib.rs`, `dyapp.udl` | library prototype | UniFFI surface over the crates above |
| Bootstrap server | `rust/bootstrap/src/service.rs`, `storage.rs` | library prototype | Answers `/dyapp/node`, `/dyapp/profile` and `/dyapp/mailbox` requests, SQLite storage, rate limiter (`rate_limit.rs`), Reed-Solomon helpers (`replication.rs`); profiles must be signed by their owner, mailbox reads by the device; no push or replication |
| Node | `rust/bootstrap/src/bin/dyappd.rs`, `node.rs`, `config.rs` | prototype | TOML/env config, startup checks, node key; runs the libp2p node and serves the node protocol ([bootstrap](bootstrap.md#served-protocol)) |
| Test peer | `rust/bootstrap/src/bin/test-peer.rs` | prototype | libp2p client CLI for the node protocol; used by network tests |
| Android app | `android/` | skeleton | `RustBridge.kt` has `System.loadLibrary` commented out |
| iOS / macOS apps | `ios/`, `macos/` | skeleton | No Rust linkage |
| Desktop | `desktop/main.rs` | skeleton | Bare Tauri shell with a `ui_test_result` command |
| Linux / Windows | `linux/`, `windows/` | skeleton | Tauri projects outside the Cargo workspace |
| Protobuf schemas | `proto/` | partial | `build.rs` in `identity`, `profile` and `p2p-net` generates `prost` types with protox (no `protoc`); `node.proto` is served for `/dyapp/node`, `/dyapp/profile` and `/dyapp/mailbox`, not `/dyapp/mailbox-push`; no gRPC/tonic |

Distinctions that matter when reading the other docs:

- **Node vs network.** `dyappd` runs a Kademlia node, but nodes do not discover each other
  or replicate yet: each is an independent server over SQLite.
- **WebRTC media vs QUIC messaging.** Video uses the `webrtc` crate (its own
  ICE/DTLS/SRTP stack); messaging is meant to run over libp2p. They are
  separate transports and share no connection.

## Target architecture (planned)

Everything in this section is design intent, not code.

```text
 Native apps (Kotlin, Swift) and Tauri desktop
        │  UniFFI
        ▼
 Rust core: identity · profile · messaging · p2p-net · video
        │                         │
        │ libp2p (QUIC, TCP)      │ WebRTC media (peer-to-peer)
        ▼                         ▼
   other peers  ◄──── signaling channel (planned, undecided)
        │
        │ libp2p node protocol (mailbox, profiles, search)
        ▼
 bootstrap nodes (dyappd + SQLite)
```

Decided (see the [ADRs](../decisions/README.md)):

- **Network stack: rust-libp2p** (Kademlia DHT, QUIC, Noise, NAT traversal)
  with subnet limits, node-ID proof-of-work and disjoint lookups against Sybil
  and eclipse attacks
  ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)). The current node has none
  of these defences.
- **Rust core, native apps on mobile and macOS, Tauri on Linux/Windows**
  ([ADR 0002](../decisions/0002-rust-core-native-apps-tauri-desktop.md)).
- **Public signed profile, end-to-end encrypted private data**
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).
- **Open bootstrap network:** anyone may run a node, a DHT is expected for
  discovery, TURN only from network nodes (no commercial relays)
  ([ADR 0007](../decisions/0007-open-bootstrap-network.md)).
- **Identity:** Peer ID = hash of a long-lived Ed25519 key
  ([ADR 0004](../decisions/0004-identity-keys.md)); end-to-end encryption with MLS
  via `openmls` [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md); deletion is best effort
  ([ADR 0011](../decisions/0011-best-effort-deletion.md)).
- **No Automerge:** signed profile versions, append-only messages ordered by a
  hybrid logical clock ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)).
  Signed profile versions are implemented (`rust/profile`); the Lamport ordering in
  `rust/messaging` is still to be replaced.
- **Delivery:** direct push, else an LRU store on bootstrap nodes; records
  replicated whole to 5 points, erasure coding K=6/M=4 only for large media
  ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).
- **Video:** one-to-one, always end-to-end, libwebrtc via FFI
  ([ADR 0013](../decisions/0013-video-calls-one-to-one.md)).

Open design choices:

- NAT traversal for messaging (hole punching, TURN nodes).
- Video signaling channel (over bootstrap, over QUIC, or other).
- Replication between bootstrap nodes.
- Local at-rest storage and its encryption on each platform.
- Authentication of bootstrap API clients.

## Data flows

**Today:** none end-to-end. Each crate is exercised by its own unit tests, and
`tests/integration_tests.rs` (registered under the bootstrap crate) combines the
library types in-process, without any network.

**Target message flow (planned):**

1. Sender appends the message to the conversation's local append-only log,
   stamps it with a hybrid logical clock and encrypts the body (`messaging`).
2. If the recipient is reachable, the message is sent over libp2p (`p2p-net`).
3. Otherwise it is stored on bootstrap nodes and fetched later by the
   recipient over `/dyapp/mailbox` (served on a single node; no app client yet).
4. The recipient acknowledges; the sender retries from its offline queue
   until then. Delivery is best effort: an LRU-evicted message is lost unless
   the sender retries.

## Fault tolerance

- **Today:** a single bootstrap process with local SQLite. No replication
  between servers, no retries, no TTL cleanup job.
- **Target:** acknowledgements, sender-side retries and store-and-forward via
  bootstrap nodes ([ADR 0009](../decisions/0009-message-delivery-and-storage.md));
  delivery stays best effort. Replication guarantees are documented in
  [Bootstrap Servers](bootstrap.md).

## Scalability

No scalability claims are made. There is no DHT, so there is no lookup
complexity to state; bootstrap capacity has not been measured.

## Roadmap

The roadmap is tracked in GitHub Issues, not in this document.

## See also

- [P2P Networking](p2p-networking.md)
- [Bootstrap Servers](bootstrap.md)
- [Encryption](../security/encryption.md)
- [Privacy and Metadata Visibility](../security/privacy.md)
