# Architecture Decision Records

An ADR records one decision that is hard to reverse, surprising without context, or a real
trade-off. Routine choices do not need one.

## Process

1. Copy [template.md](template.md) to `NNNN-short-title.md` (next free number, four digits).
2. Fill it in. Status starts as **Proposed**.
3. Merge it with the change it describes; set status to **Accepted**.
4. To change an accepted decision, update its ADR in the same change and set a new date. ADRs
   describe current decisions only; git history keeps earlier versions.

ADRs are written by hand. Agents only propose one; a person confirms it before it is added. API
reference generation is described in [documentation.md](../development/documentation.md).

## Index

| # | Title |
|---|-------|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions |
| [0002](0002-rust-core-native-apps-tauri-desktop.md) | Rust core with native mobile apps and Tauri desktop |
| [0003](0003-public-signed-profile-encrypted-private-data.md) | Public signed profile, end-to-end encrypted private data |
| [0004](0004-identity-keys.md) | Long-lived Ed25519 identity key |
| [0005](0005-openmls-end-to-end-encryption.md) | End-to-end encryption with MLS (openmls) |
| [0006](0006-transport-keys-separate-from-identity.md) | Transport keys are separate from the user's identity key |
| [0007](0007-open-bootstrap-network.md) | Open bootstrap network with DHT and no commercial relays |
| [0008](0008-sybil-and-eclipse-defences.md) | rust-libp2p with layered Sybil and eclipse defences |
| [0009](0009-message-delivery-and-storage.md) | Message delivery, offline storage and replication |
| [0010](0010-data-sync-without-automerge.md) | Data sync without Automerge: signed profile versions and HLC-ordered messages |
| [0011](0011-best-effort-deletion.md) | Deletion is a signed request, honoured best effort |
| [0012](0012-private-p2p-interactions.md) | Interactions between users are private P2P signals |
| [0013](0013-video-calls-one-to-one.md) | One-to-one video calls over libwebrtc, always end-to-end |

All listed ADRs are accepted.
