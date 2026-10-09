# Replication Design

**Status:** approved design (2026-10-09), **planned**. What runs today is in
[Bootstrap: Replication and repair](bootstrap.md#replication-and-repair): clients write messages
and profiles to 5 nodes themselves, nodes repair mailboxes, and media are not replicated. This page
is the target; as each step below lands, the bootstrap page and
[ADR 0009](../decisions/0009-message-delivery-and-storage.md) take over its text and this page
shrinks.

Replica keys are `replica_key(key, i) = H(key‖i)`, `i` in `0..REPLICAS` (`REPLICAS` = 5,
`rust/p2p-net/src/lib.rs`). "The node of key k" is the node closest to k in the DHT.

## Write path

### Messages, MLS commits, signals and profiles

1. The client sends one put to the node of replica key 0. If that fails, it tries key 1, then 2,
   and so on.
2. That node (the acceptor) verifies the record as it does now and stores it. It then looks up the
   nodes of the other 4 replica keys and sends each a `replica_put`. A receiver verifies a
   `replica_put` like a client put. No node trusts another node.
3. The acceptor answers `ok` once 2 of 5 copies are stored: its own and one other. The other
   copies are sent in the background.
4. A single-node network answers `ok` after the local copy.

Profiles get a `replica_put` like the one mailboxes already have
(`rust/bootstrap/src/service.rs`). `test-peer` stops writing replicas itself.

### Media

- The client sends a whole blob in one `put`. The protocol maximum is **6 MiB**
  (`max_media_bytes` in `info`). The client splits a larger file into blobs, and the message lists
  their hashes.
- A node stores no object (whole blob, manifest or shard) larger than **1 MiB**.
- The acceptor picks the layout by its own config key `media.shard_threshold`. The default is
  1 MiB, and a value above 1 MiB is rejected at startup.
  - **Size ≤ threshold:** whole copies at R=5, on the nodes of `replica_key(hash, i)`, written the
    same way as messages.
  - **Size > threshold:** Reed-Solomon with K = ⌈size / 1 MiB⌉ data shards (so K ≤ 6) and
    M = 4 parity shards; the protocol requires M ≥ 4. Shard j goes to the node of
    `replica_key(hash, j)`, for j in `0..K+M`.
- A **manifest** holds K, M, the blob length and the SHA-256 of every shard. It is stored at R=5
  on the blob's own replica keys. The manifest is what marks an object as sharded, so every stored
  object describes itself and the threshold need not match across nodes. Only the 6 MiB and 1 MiB
  limits are protocol constants.
- The acceptor answers `ok` once the manifest (2 of 5 copies) and K shards are stored. The other
  M shards are sent in the background.
- New `/dyapp/media` node-to-node requests: `replica_put(blob | manifest)`,
  `shard_put(hash, j, shard)` and `shard_get(hash, j)`. A shard receiver checks the shard against
  the hash listed in the manifest, which it fetches or which is sent with the shard. It accepts the
  shard only if the owner's latest `keep` lists the blob, as for a client `put`.

### Abuse controls

- The originating client pays for the fan-out. A message or profile put costs 5 token-bucket
  units. A media put costs bytes × 5 for whole copies or bytes × (K+M)/K for shards, against the
  per-owner media quota and `bytes_per_second`.
- A `replica_put`, `shard_put` or `shard_get` from a node counts against that node's peer limits.
- A dishonest acceptor that skips the fan-out or writes a fake manifest is caught three ways:
  owner-presence repair below, the sender's retry queue, and the hash check after decoding.

## Read path

- **Mailbox:** unchanged. The node pushes to a watching device, and on a miss the client tries the
  next replica key.
- **Profile:** get from the node of replica key 0; on a miss try the next key. The highest version
  wins.
- **Media:** get from the node of any replica key. If that node holds the whole blob, it returns
  it. If it holds a manifest, it fetches K shards with `shard_get`, decodes them, checks the blob
  hash and returns the whole blob. The client checks the hash too. Ranged `get(hash, range)` is
  planned on top of the manifest.

## Repair

Repair runs only while the owner is present, and nodes do the work. Data of an owner who stays
offline is not repaired and expires by its TTL.

- **Mailbox:** unchanged. A node that serves a fetch sends an inventory to the other replicas and
  fills the gaps (`rust/bootstrap/src/node.rs`).
- **Profile:** with each publish or heartbeat, the client's node compares the version held by the
  other 4 replicas and sends the record to any that are missing it or hold an older version.
- **Media:** with each `keep`, the node that takes it checks every listed hash. For whole copies
  it copies the blob to replicas that lack it. For sharded blobs it fetches K shards and rebuilds
  the missing ones. A blob with fewer than K shards left (or no copy left) is listed in the `keep`
  reply, and the client uploads it again.
- **Budget:** repair and media traffic together use at most 75 % of `bytes_per_second`. A node
  takes an inventory of a key at most once an hour.

## Implementation steps

Each step is one change with multi-node Docker tests and its docs.

1. Mailbox and profile fan-out by the acceptor with the 2 of 5 ack; profile `replica_put`; client
   billed ×5; `test-peer` stops writing replicas.
2. Profile repair through node inventory.
3. Media: 6 MiB intake; the manifest and the shard requests in `proto/node.proto`; whole copies at
   R=5 or RS with K = ⌈size / 1 MiB⌉, M = 4, using `rust/bootstrap/src/replication.rs`; decoding
   on `get`; `media.shard_threshold`.
4. Media repair by `keep`.
5. Docs: ADR 0009 edited in place, without its stale line that presence-driven repair does not
   exist; [Bootstrap](bootstrap.md) sections Replication and repair, Erasure coding and Protocol;
   [Deployment](../operations/deployment.md) for the new config key. This page is then removed.

Ranged media get, signal stores and the media size limits in config build on the manifest and the
threshold.
