# 0009. Message delivery, offline storage and replication

- **Status:** Accepted
- **Date:** 2026-10-09

## Context

Peers are often offline, so bootstrap nodes must hold data for them. Offline messages are a few
kilobytes, and rust-libp2p Kademlia already stores whole records on the closest nodes; splitting
such records into shards would mean many DHT lookups per message and our own rebuild code. Large
media is different: full copies are expensive, and a node should hold no object larger than
1 MiB. A client that writes every replica itself pays the upload 5 times over a mobile link.

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
- **Write tokens:** the recipient gives each contact, inside MLS, a daily write token signed by
  its device key; envelopes with a token use the mailbox quota, envelopes without one (a first
  message) a small pool per mailbox evicted by the peer that handed them in and the sender's IP
  group. A blocked contact's token keys are revoked by a signed `revoke` that replicas forward
  and repair. Details: [Bootstrap: Mailbox envelope](../architecture/bootstrap.md#mailbox-envelope).
- **History lives on devices.** A new device gets it from a linked device
  ([ADR 0004](0004-identity-keys.md)), not from bootstrap.
- **Messages, MLS commits and profiles are replicated whole** to R = 5 points, replica *i* at the
  nodes closest to H(key ‖ i) ([ADR 0008](0008-sybil-and-eclipse-defences.md)). A record survives
  the loss of any 4 replicas, and hiding it needs 5 eclipsed points.
- **The node fans out, not the client:** the client sends one put; the node that takes it stores
  the copies for the replica keys it holds, sends the rest to their holders and answers once
  2 copies are stored. Receivers verify every copy themselves. The client pays for the fan-out.
- **Media:** a blob is at most 6 MiB. Up to the node's threshold (at most 1 MiB) it is stored as
  5 whole copies; above it as Reed-Solomon shards, K = ⌈size / 1 MiB⌉ data (at most 6) and
  M = 4 parity, under a manifest stored at R = 5. Copies and shards carry the owner's signed
  `keep`, so receivers accept only blobs the owner listed.
- **Repair while the owner is present, by nodes:** a fetch, a profile publish or a media `keep`
  makes the node compare the replica holders and fill the gaps. A repaired copy keeps the
  original expiry or owner age, so repair never extends a TTL. Data of an absent owner is not
  repaired and expires by TTL.
- Details: [Bootstrap: Replication and repair](../architecture/bootstrap.md#replication-and-repair).
- All stored message data is end-to-end encrypted
  ([ADR 0005](0005-openmls-end-to-end-encryption.md)).

## Consequences

- A message to a long-inactive user can be evicted before delivery; the sender's retry queue is the
  fallback. Cache size per node is open; protection against eviction by spam is in
  [ADR 0008](0008-sybil-and-eclipse-defences.md).
- Storage overhead for messages and small media is 5×, for sharded media (K+M)/K, at most 5×.
- A network under 10 nodes puts several shards on one node and tolerates fewer losses.
- `ok` means 2 copies; the other 3 rely on the acceptor finishing or on repair, so an owner who
  never comes back may keep only 2.
- Implemented: envelopes expire after 24 hours, profiles and tombstones 30 days after the
  owner's last newer publish or fresh signed heartbeat. Not implemented: eviction by profile
  activity (a full store evicts its oldest records: profiles by last publish, envelopes by arrival);
  acceptors use full DHT lookups of `dyapp_p2p_net::replica_key` for mailbox/profile fan-out
  and wait for two distinct stored copies (one only with no routing peers), within 10 seconds
  and a 64-write cap. `dyappd put` uploads once; the `dyapp-client` crate (`rust/client`) puts each MLS Welcome, commit and padded message once per member device mailbox, fetches and acks after sealing its state, and the app does not use it yet. Nodes push to
  watching devices, forward acks and repair mailboxes on watching fetch. Profile publish and
  heartbeat trigger version inventories and fill older or missing holders, at most once per
  rolling hour per key and within the 75 % repair budget. Acceptors store media up to the node's
  threshold as whole copies and larger blobs up to 6 MiB as Reed-Solomon shards under a
  manifest; a signed `keep` repairs missing copies, manifests and shards on the current holders.
  Nodes check write tokens and revokes and keep the tokenless pool; the pool binds only under
  `limits.require_write_tokens` (off by default), and clients do not issue tokens yet.
  Planned: ranged media reads with an assembled-blob cache, and a holder that is no longer
  closest to a replica key pushing the object on.
