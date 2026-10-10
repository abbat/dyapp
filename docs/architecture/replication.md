# Replication Design

**Status:** approved design (2026-10-09), **planned**. What runs today is in
[Bootstrap: Replication and repair](bootstrap.md#replication-and-repair): acceptors replicate
messages and profiles with a two-holder acknowledgement, nodes repair mailboxes and profiles, and media are
not replicated. This page
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

- The client sends a whole blob in one `put`. The protocol maximum is **6 MiB**
  (`max_media_bytes` in `info`). The client splits a larger file into blobs, and the message lists
  their hashes.
- A node stores no object (whole blob, manifest or shard) larger than **1 MiB**.
- The acceptor picks the layout by its own config key `media.shard_threshold`. The default is
  1 MiB, and a value above 1 MiB is rejected at startup.
  - **Size ≤ threshold:** whole copies at R=5 on the holders of `replica_key(hash, i)`, written as
    in the steps above.
  - **Size > threshold:** Reed-Solomon with K = ⌈size / 1 MiB⌉ data shards (so K ≤ 6) and M = 4
    parity shards; the protocol requires M ≥ 4. Shard j goes to the holder of
    `replica_key(hash, j)`, for j in `0..K+M`.
- A **manifest** holds the blob length and the SHA-256 of every shard. K and M follow from the
  length, so the manifest of a blob is unique: a node keeps the first one it gets and answers
  `invalid` to a different one. It is stored at R=5 on the blob's own replica keys. The manifest
  is what marks an object as sharded, so every stored object describes itself and the threshold
  need not match across nodes. Only the 6 MiB and 1 MiB limits are protocol constants.
- A whole blob wins: a node that holds a whole copy whose SHA-256 matches ignores manifests for
  that hash.
- The acceptor answers `ok` once the manifest (2 copies) and K shards are stored. The other M
  shards are sent in the background.
- New `/dyapp/media` node-to-node requests: `replica_put(blob | manifest)`,
  `shard_put(hash, j, shard)` and `shard_get(hash, j)`. A `replica_put` or `shard_put` carries the
  owner's signed `keep`, or the sender's signed attachment for chat media. The receiver verifies
  the signature itself and stores the `keep` only if it is newer than the one it holds. It
  accepts the object only if the hash is listed in that owner's latest `keep`, so an old `keep`
  cannot bring back a deleted blob, and charges it to that owner's quota. A shard must match the
  hash in the manifest.
- Small networks: a node may hold several copies or shards of one blob (keyed by hash and j), and
  a copy that lands on a node that already has it counts as stored. With fewer than 10 distinct
  holders the loss tolerance drops; `dyappd status` shows how many distinct nodes hold replicas.

### Limits and abuse controls

- Implemented: at most 2 media requests per connection and 16 inbound requests per node run
  at once. Excess streams close before the body is read, without a status reply; typed clients
  report a transport failure and retry with backoff. See
  [Resource guards](bootstrap.md#resource-guards).
- The originating client pays for the fan-out. A message or profile put costs 5 token-bucket
  units. A media put costs bytes × 5 for whole copies or bytes × (K+M)/K for shards, against the
  per-owner media quota and `bytes_per_second`.
- A media `get` costs the requester the bytes of the reply, not one unit.
- A `replica_put`, `shard_put` or `shard_get` from a node counts against that node's peer limits.
- A dishonest acceptor that skips the fan-out or writes a fake manifest is caught three ways:
  owner-presence repair below, the sender's retry queue, and the hash check after decoding.

## Read path

- **Mailbox:** unchanged. The node pushes to a watching device, and on a miss the client tries the
  next replica key.
- **Profile:** get from the holder of replica key 0; on a miss try the next key. The highest
  version wins.
- **Media:** get from the holder of any replica key. If it holds the whole blob, it returns it. If
  it holds a manifest, it fetches K shards with `shard_get`, decodes them, checks the blob's
  SHA-256 and returns the whole blob. On a mismatch it deletes the manifest and its shards and
  answers `not_found`. The client checks the hash too. Assembly comes out of the node's repair
  budget below. Planned on top of the manifest: `get(hash, offset, len)` answers one
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
- **Media:** with each `keep`, the node that takes it checks up to 256 of the listed hashes, from a
  cursor that moves on with each `keep`. It groups (hash, j) by holder and sends each holder one
  `have(list)`, which answers `missing(list)`, so the request count follows the number of holders,
  not hashes. A node answers `have` only to a trusted peer its routing table places near the key,
  as for mailbox inventories, so it is not an oracle of stored hashes. Only what is missing is
  fetched: a whole copy is copied, a missing shard is rebuilt from K others and the decode checks
  the blob's SHA-256. A blob with fewer than K shards left (or no copy left) is listed in the
  `keep` reply, and the client uploads it again.
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
4. Media whole copies: `replica_put(blob)` carrying the signed `keep` or attachment,
   `get` billed by bytes, distinct holders in `dyappd status`. Stream concurrency limits are
   implemented; the remaining write path is planned.
5. Media shards: 6 MiB intake, the manifest and the shard requests in `proto/node.proto`, RS with
   K = ⌈size / 1 MiB⌉, M = 4 using `rust/bootstrap/src/replication.rs`, decoding on `get`,
   `media.shard_threshold`.
6. Media repair by `keep`, batched per holder.
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
