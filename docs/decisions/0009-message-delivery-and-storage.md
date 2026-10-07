# 0009. Message delivery, offline storage and replication

- **Status:** Accepted
- **Date:** 2026-10-08

## Context

Peers are often offline, so bootstrap nodes must hold data for them. Offline messages are a few
kilobytes, and rust-libp2p Kademlia already stores whole records on the closest nodes; splitting
such records into shards would mean many DHT lookups per message and our own rebuild code. Large
media is different: full copies are expensive.

## Decision

- **Delivery through mailboxes, one per device:** each device is a separate MLS member and has its
  own mailbox, its replica nodes; a message is always written to the mailbox of every recipient
  device, so one device's ack never removes another device's copy. An online device keeps a
  connection to one of its replica nodes and gets new messages pushed over it at once. Clients do
  not publish their addresses in the DHT; a direct connection that already exists (for example
  during a call) is only an optimisation. The device acknowledges, the node forwards the ack to
  the other replicas; the sender keeps an offline queue and retries.
- **Bootstrap storage is a cache, nothing is kept forever:** the node operator sets a retention
  TTL (default 30 days) after which old data, tombstones included, is deleted, and may delete any
  data at any time. Under a full quota the data of the profiles inactive longest is evicted first.
  A user who comes online again restores their data on the nodes.
- **History lives on devices.** A new device gets it from a linked device
  ([ADR 0004](0004-identity-keys.md)), not from bootstrap.
- **Messages, MLS commits and profiles are replicated whole** to R = 5 points, replica *i* at the
  nodes closest to H(key ‖ i) ([ADR 0008](0008-sybil-and-eclipse-defences.md)). A record survives
  the loss of any 4 replicas, and hiding it needs 5 eclipsed points.
- **Erasure coding only for large media** such as photos: K=6 data and M=4 parity shards, fixed by
  the protocol (survives the loss of any 4 of 10 shards).
- All stored message data is end-to-end encrypted
  ([ADR 0005](0005-openmls-end-to-end-encryption.md)).

## Consequences

- A message to a long-inactive user can be evicted before delivery; the sender's retry queue is the
  fallback. Cache size per node is open; protection against eviction by spam is in
  [ADR 0008](0008-sybil-and-eclipse-defences.md).
- Storage overhead for messages is 5×.
- Not implemented: the bootstrap stores records with an unenforced TTL field;
  `rust/bootstrap/src/replication.rs` is a local Reed-Solomon codec and nothing replicates.
