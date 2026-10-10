# Replication Design

**Status:** approved design (2026-10-09), **planned**. What runs today is in
[Bootstrap: Replication and repair](bootstrap.md#replication-and-repair): acceptors replicate
messages, profiles and whole-copy media with a two-holder acknowledgement, and media up to
6 MiB with Reed-Solomon shards. Nodes repair mailboxes, profiles and media on owner presence. This page
is the target; as each step below lands, the bootstrap page and
[ADR 0009](../decisions/0009-message-delivery-and-storage.md) take over its text and this page
shrinks.

Replica keys are `replica_key(key, i) = H(key‖i)`, `i` in `0..REPLICAS` (`REPLICAS` = 5,
`rust/p2p-net/src/lib.rs`). "The holder of key k" is the node closest to k in the DHT.

## Write path

### Messages, MLS commits, signals and profiles

Implemented; the write path, two distinct confirmations, 10-second deadline, 64 active fan-outs,
profile `replica_put`, and single-upload `test-peer` commands are documented in
[Bootstrap](bootstrap.md#replication-and-repair).

### Media

Whole-copy and Reed-Solomon media replication up to 6 MiB, signed authorization,
byte billing, manifests, shard validation and decoding are implemented; see
[Bootstrap](bootstrap.md#replication-and-repair) and [Erasure coding](bootstrap.md#erasure-coding-reed-solomon).
Keep-driven media repair is implemented. Ranged reads and assembly caching remain planned.

### Limits and abuse controls

- Implemented: at most 2 media requests per connection and 16 inbound requests per node run
  at once. Excess streams close before the body is read, without a status reply; typed clients
  report a transport failure and retry with backoff. See
  [Resource guards](bootstrap.md#resource-guards).
- Whole-copy and sharded upload billing, byte-billed gets, shard request limits and bounded
  assembly are implemented; see [Bootstrap](bootstrap.md#erasure-coding-reed-solomon).
- A dishonest acceptor that skips the fan-out or writes a fake manifest is caught three ways:
  owner-presence repair below, the sender's retry queue, and the hash check after decoding.

## Read path

- **Mailbox:** unchanged. The node pushes to a watching device, and on a miss the client tries the
  next replica key.
- **Profile:** get from the holder of replica key 0; on a miss try the next key. The highest
  version wins.
- **Media:** whole reads and verified assembly are implemented; see
  [Bootstrap](bootstrap.md#erasure-coding-reed-solomon). Planned on top of the manifest: `get(hash, offset, len)` answers one
  piece of at most 1 MiB with the blob's total size, so a client fetches large blobs piece by
  piece and resumes after a break; without `len` it is the whole get. A blob assembled from
  shards is kept as `<media dir>/cache/<hash>` for `limits.media_cache_minutes` (default 10)
  after its last read, so the pieces read that file instead of assembling again. The cache
  counts towards `media_max_mb` and is evicted first; the hourly cleanup deletes expired
  entries. Piece bytes count towards the traffic cap like a get; the client checks the hash
  once it has all pieces.

## Repair

Repair runs only while the owner is present, and nodes do the work. Data of an owner who stays
offline is not repaired and expires by its TTL.

- **Mailbox:** unchanged. A node that serves a fetch sends an inventory to the other replicas and
  fills the gaps (`rust/bootstrap/src/node.rs`).
- **Profile:** implemented; version inventories, rolling hourly limits and byte reservations are
  described in [Bootstrap](bootstrap.md#replication-and-repair).
- **Media:** keep-driven bounded inventories, trusted-neighbour checks, missing-item repair,
  verified decoding and unrecoverable hashes in the keep reply are implemented; see
  [Bootstrap](bootstrap.md#replication-and-repair).
- **Network growth:** when new nodes join, the holder of some replica keys changes and the new
  holder has nothing yet. A read there misses and the client tries the next key. Repair checks the
  current holders, so for a present owner it moves the data to them. For an owner who stays
  offline, data stays on the old holders until its TTL; a holder that sees it is no longer closest
  and pushes the object on is planned.
- **Budget:** repair and media assembly together use at most 75 % of `bytes_per_second`. A node
  takes an inventory of a key at most once an hour.

## Implementation steps

Each step is one change with multi-node Docker tests and its docs.

1. Per-protocol size limits are implemented; see [Served protocol](bootstrap.md#served-protocol).
2. Mailbox and profile acceptor fan-out is implemented; see
   [Replication and repair](bootstrap.md#replication-and-repair).
3. Profile repair through node inventory is implemented; see [Bootstrap](bootstrap.md#replication-and-repair).
4. Media whole copies and stream concurrency limits are implemented; see
   [Bootstrap](bootstrap.md#replication-and-repair).
5. Media shards, 6 MiB intake, verified assembly and `media.shard_threshold` are implemented;
   see [Bootstrap](bootstrap.md#erasure-coding-reed-solomon).
6. Media repair by `keep`, batched per holder, is implemented; see
   [Bootstrap](bootstrap.md#replication-and-repair).
7. Docs: ADR 0009 edited in place, without its stale line that presence-driven repair does not
   exist; [Bootstrap](bootstrap.md) sections Replication and repair, Erasure coding and Protocol;
   [Deployment](../operations/deployment.md) for the new config key. This page is then removed.

Ranged media get and its cache build on the manifest and the threshold.

## Stress Test Results

### Resolved decisions
- Wire limit: per-protocol limits instead of one global one, so 6 MiB media do not let every
  protocol buffer 6 MiB per stream.
- Replica authorization: the owner's signed `keep` (or the sender's attachment) travels with each
  media copy; receivers verify it themselves.
- Manifest: deterministic from the length, first one wins, a whole blob wins, every decode checks
  SHA-256. Only the owner or the sender can name a hash, so outsiders cannot plant a manifest.
- Small networks: copies and shards co-locate, status shows distinct holders.
- Network growth: present owners are rebalanced by repair; holder-side republish for offline
  owners is planned.
- Read amplification: `get` is billed by bytes, assembly uses the repair budget, no cache.
- Acceptor crash after `ok`: no fan-out queue on disk, repair closes it; `ok` with one copy only
  with no peers at all.
- Repair cost: batched inventory per holder, 256 hashes per `keep`.
- Acceptor role: stores only for keys it holds, otherwise forwards.
- Plan review: fan-out runs in the node's swarm loop with the reply held until 2 distinct holders
  store; a `replica_put` is never forwarded; excess media streams are reset instead of answered
  `rate_limited`; replicas are accepted only by the owner's latest `keep`; `have` is answered
  only to near trusted peers; steps split into 7; no mixed-version rollout before release.

### Changes made
All of the above were folded into the sections of this page.

### Deferred
- Holder-side republish when a node stops being closest to a key.
- A cache of decoded blobs, if load shows the need.

### Confidence
- Overall: high.
- Concern: durability in networks under 10 nodes is low by construction, not by design choice.
