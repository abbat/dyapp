# 0006. Transport keys are separate from the user's identity key

- **Status:** Accepted
- **Date:** 2026-10-07

## Context

rust-libp2p ([ADR 0008](0008-sybil-and-eclipse-defences.md)) authenticates every connection with
the libp2p node key (TLS 1.3 over QUIC, Noise over TCP). MLS
([ADR 0005](0005-openmls-end-to-end-encryption.md)) authenticates users end to end: each device
signs with its own key, certified by the identity key.

Using the identity key in the transport would show it to every node a device connects to,
including DHT nodes that never talk to the user. Anyone could then tell when and from which IP
address a given user is online, and one key would serve two purposes (transport handshakes and
user signatures), which key separation forbids.

## Decision

- **The transport authenticates only the libp2p key of the device**, a key used for nothing else.
  Transport security is libp2p's own; it protects each hop, not the conversation.
- **The user's Peer ID is proven end to end**, never by the transport: by MLS credentials
  ([ADR 0005](0005-openmls-end-to-end-encryption.md)) and by the signature on the public profile
  ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)). A client trusts content only
  after that check, whichever node delivered it.
- **Video:** the call offer and answer, with their DTLS fingerprints, travel as MLS messages of the
  chat's group, so MLS authenticates the fingerprints and a relay cannot substitute its own
  ([ADR 0013](0013-video-calls-one-to-one.md)).
- Devices that only use the network (phones) may rotate their libp2p key; index nodes keep theirs
  for the node-ID proof-of-work ([ADR 0008](0008-sybil-and-eclipse-defences.md)).

## Consequences

- A node on the path learns a device's network key and IP address, not which user it belongs to.
- How a contact learns a device's current libp2p address without revealing presence to everyone is
  open ([ADR 0012](0012-private-p2p-interactions.md)).
- Not implemented: `rust/p2p-net` has no transport security yet.
