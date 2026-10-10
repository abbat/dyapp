# Bootstrap Server Architecture (Distributed Relay)

## Overview

A bootstrap node (`dyappd`) serves owner-signed profiles and per-device mailboxes over
libp2p ([Served protocol](#served-protocol)), pushes new envelopes to watching devices and
repairs mailbox replicas node to node; profile search and signaling are planned.

> ⚠️ The server stores whatever bytes clients send as envelope ciphertext; no client encrypts
> yet. Profile writes need the owner's signature; mailbox reads and deletes need the device's.
> See [Encryption & Security Status](../security/encryption.md).

**Key principles:**
- **Replication**: messages and profiles replicated whole to 5 points, Reed-Solomon K ≤ 6, M = 4 only for media above 1 MiB [ADR 0009](../decisions/0009-message-delivery-and-storage.md); the client writes each replica to the node the DHT finds, nodes forward mailbox acks and repair mailboxes; media are not replicated and the Reed-Solomon codec is not wired in (see [Replication and repair](#replication-and-repair), [Erasure coding](#erasure-coding-reed-solomon))
- **Mailboxes**: one per device, read and emptied only with the device key's signature; envelopes expire after a TTL (default 24h, deleted hourly; see [Privacy](../security/privacy.md#retention))
- **Profile storage**: public profiles signed by the owner's identity key; the highest version wins and deletion is a signed tombstone ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); no search endpoint yet
- **Rate limiting**: per-peer token bucket on profile and mailbox requests (see [Rate Limiting](#rate-limiting))
- **Encryption (target)**: clients end-to-end encrypt messages and media before upload; profiles are public and signed, not encrypted ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); not implemented
- **Network**: open, anyone may run a node, Kademlia DHT discovery ([ADR 0007](../decisions/0007-open-bootstrap-network.md), [P2P networking](p2p-networking.md)); storage is a cache with an operator-set retention TTL (default 30 days) and eviction by profile activity ([ADR 0009](../decisions/0009-message-delivery-and-storage.md))

## Target Design — planned

Served today ([Served protocol](#served-protocol)): `/dyapp/node`, `/dyapp/profile`,
`/dyapp/mailbox`, `/dyapp/mailbox-push` and `/dyapp/media`, the store, media and TURN roles,
mailbox replicas and repair over the DHT. Signals in the mailbox (the envelope class and the
signal quota), the search service (gossipsub, `/dyapp/search-kad`, `profiles-idx.db`), media
replication and retention, profile proof of work
and the store format path are planned; each part below says which it is, and the sections after
it describe today's code.

### Principles

- **libp2p only.** Clients and nodes exchange protobuf messages over libp2p request-response,
  one protocol ID per service, schemas in `proto/`; no REST, no gRPC
  ([ADR 0014](../decisions/0014-libp2p-only-node-protocol.md)). Call media goes over WebRTC.
- **Version-agnostic network.** Protocol IDs carry no version and nodes negotiate none. Messages
  evolve only by new protobuf fields; unknown fields are ignored and kept in signed payloads. A
  node states what it supports (limits, filters) in its replies.
- **The signed record is the source of truth.** A node stores the owner-signed payload as
  received and serves it unchanged, so an old node still carries fields it does not understand.
- **Nothing is kept forever.** Every store is a cache: the operator sets a retention TTL
  (default 30 days, tombstones included) and may delete any data at any time. A user who comes
  online again restores their data. No backups: a rebuilt node joins empty.
- **Design changes never stop service.** Nodes upgrade one at a time and old and new versions
  serve side by side. A changed store or index format is built next to the old one, in the
  background under the I/O budget, while the old one keeps answering; the node switches when the
  new one is ready and then deletes the old one.
- **Clients write, nodes store.** A client writes every replica or shard itself; nodes do not
  fan out writes for others.
- **Nodes see metadata, not content.** Messages, likes, views and every other signal are
  end-to-end encrypted for the recipient. Who writes to whom and when stays visible to nodes; a
  sealed sender may come later.

### Roles and discovery

Each role is enabled separately in the node config:

| Role | Holds |
|------|-------|
| store | signed profiles, per-device mailboxes (messages, likes, views and other signals) |
| media | media blobs and erasure-coded shards, thumbnails |
| search | the search index over profiles collected from the whole network |
| TURN | short-lived credentials for an operator-run coturn relay of call media |

Every role has its own DHT key space, so replicas for a role are chosen only among the nodes
that run it. A node announces its roles through libp2p identify. A reachable node is a DHT
server; a node behind NAT is a bootstrap only for peers in its local network. Mobile clients run
the DHT in client mode: they store nothing and answer no DHT queries. Today `/dyapp/kad` is the
store role's key space: `dyappd` without the store role runs it in client mode and serves no
protocol. The media role needs the store role and shares its key space for now; other roles get
their own Kademlia protocol name when they are built.

Kademlia gives every node an equal share of keys, and v1 does not weight it by capacity: a weak
node refuses or evicts what exceeds its limits and the other replicas keep the data. Several
virtual positions per node would each need a proof of work and would open a Sybil gap.

### Protocol

One libp2p request-response protocol per service, protobuf requests and replies, schemas in
`proto/`. `proto/node.proto` defines `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`,
`/dyapp/mailbox-push` and `/dyapp/media` ([schema](protobuf-schema.md#node-protocol)); the other services get their
schemas with their roles. Each request is a `oneof`; a node that gets a variant it does not know answers
`unsupported` and the client tries another node. Every reply carries a status: `ok`, `not_found`,
`stale`, `too_large`, `full`, `rate_limited`, `denied`, `unsupported`, `invalid`, `refused`.

| Protocol | Role | Requests |
|----------|------|----------|
| `/dyapp/node` | all | `info`: roles, limits (payload, message, mailbox, media), supported search filters, minimum profile proof of work, retention TTL |
| `/dyapp/profile` | store | `publish(SignedRecord)`, `get(identity)`, `heartbeat(SignedRecord)` |
| `/dyapp/mailbox` | store | `challenge`, `put(envelope)`, `fetch(mailbox)`, `ack(ids)`; node to node `replica_ack`, `inventory(ids)` → `missing(ids)`, `replica_put(envelopes)` |
| `/dyapp/mailbox-push` | client | the node pushes new envelopes to a connected device over its connection |
| `/dyapp/media` | media | `keep(signed hash list)`, `put(owner, blob)`, `get(hash)`, `attach(signed chat attachment)`, `release(secret)`; `get(hash, offset, len)` is planned |
| `/dyapp/search` | search | `publish(SignedRecord)`, `heartbeat(SignedRecord)`; `query(conditions, limit)` → (identity key, version) pairs in random order, the conditions applied, `partial`; `get(keys)` → `SignedRecord`s |
| `/dyapp/inventory` | search | node to node: `have(topic, (key, version) list)` → `need(list)`, for search catch-up |
| `/dyapp/node` | TURN | `turn` → short-lived username, password and URLs of the node's coturn |

Gossipsub topics `/dyapp/profiles/<n>`, n = H(identity) mod 16, carry `SignedRecord`s and
heartbeats between search nodes only ([Search](#search)); the search role has its own Kademlia,
`/dyapp/search-kad`. Media and other large payloads go in chunks below the node's
request size limit.

**Authorisation.** The Noise peer ID is a transport key, never an identity
([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)), so anything that acts
on an identity's data carries a signature by a key of that identity:

| Request | Signed by | Checked against |
|---------|-----------|-----------------|
| `profile.publish` | owner identity key, over the record | the key in the record |
| `mailbox.fetch`, `mailbox.ack` | the mailbox's device key, over the request and a `challenge` nonce bound to this connection | mailbox address = H(device key) |
| `mailbox.put` | sender's device key, over the envelope | per-key and per-IP-group quotas only; the recipient checks the sender inside the MLS ciphertext |
| `media.keep` | owner identity key, over the versioned list of the owner's hashes | the key; per-owner media quota |
| `media.put` | nothing: the node takes only a blob whose hash the owner's latest `keep` lists | the list and the quota |
| `media.attach` | sender identity key, over the attachment's hashes, release hash and creation time | the key; per-owner media quota and attachment count |
| `media.release` | nothing: the release secret, which only the sender and the recipient know | SHA-256 of the secret against the attachment's release hash |
| `search.publish`, `search.heartbeat` | owner identity key, over the record | the key in the record and the profile proof of work |
| `search.query`, `search.get`, `node.turn`, `profile.get`, `media.get` | nothing | rate limit per peer ID and IP group |

An ack is signed by the device, so a node forwards it verbatim and the other replicas verify it
themselves; no node trusts another node. Requests other than fetch and ack are idempotent (the
same id or hash stores once), so replays need no nonce.

### Replication and repair

Profiles and mailbox envelopes (signals included) are replicated whole to R = 5 points,
replica *i* on the nodes closest to H(key ‖ i); media above the node's threshold are to be
erasure-coded into K = ⌈size / 1 MiB⌉ + M = 4 shards, smaller ones such as thumbnails stored whole
([ADR 0009](../decisions/0009-message-delivery-and-storage.md), planned:
[Replication design](replication.md)).
`dyapp_p2p_net::replica_key(key, i)` is the Kademlia lookup key `key ‖ i` (Kademlia applies
SHA-256). The client writes each replica itself; a node stores only what it is sent, and a
repeated put is a no-op, so a duplicate or retried replica write is harmless. No client does the
replica lookup yet.

Repair is driven by the owner's presence; mailbox repair is implemented, profile and media
repair wait for a client that does the replica lookup. When a user comes online, the nodes
responsible for their keys compare inventories, *I have* and *I need*, and fill the gaps, so new
closest nodes get the data after churn and stale replicas catch up. Data of a user who stays
offline is not repaired and expires with the TTL; a message whose replicas are all lost is
resent from the sender's retry queue.

- **Profile.** The owner's client checks its profile version on each replica when it comes
  online and republishes where the replica is missing or older; a repeated publish answers
  `STALE` and stores nothing.
- **Media.** The owner's client sends its `keep` list to each replica; the node answers the
  missing hashes and the client re-uploads them. Nodes never rebuild media or erasure-coded
  shards.
- **Mailbox.** A watching fetch makes the node the repairer for that mailbox: it sends the ids it
  holds to the closest node of each other replica key in a `mailbox.inventory` request. The
  peer answers the ids it lacks, and the repairer sends those envelopes in one `replica_put`,
  each verified like a put. The peer sends the envelopes the repairer lacks back only
  if its own routing table places the repairer among the closest nodes of one of the mailbox's
  replica keys, so an arbitrary peer cannot pull a mailbox's ciphertexts.
- **No grace period.** Nothing is copied when a node leaves or restarts; only the owner's next
  visit moves data, and only the gaps, so a restart never moves the node's whole store.
- **Budget.** One inventory per mailbox per hour on a node; repair traffic counts towards
  `bytes_per_second` and stops at 75 % of it, together with media.
- **Scale.** 120 million users on 1200 nodes with R = 5 put about 500 000 users and 750 000
  device mailboxes (1.5 devices per user) on a node. If half the devices come online daily, a
  node sends about 4 inventories a second to 4 peers each, a few hundred bytes apiece (16-byte
  ids of pending messages); envelopes move only for real gaps.

### Profiles

- Payload at most 1 MiB, a protocol constant like every record size limit
  ([Limits and abuse](#limits-and-abuse)). Media are separate blobs; the profile only links to
  them by content hash.
- Each photo in `photos` is the full image (one or more blobs) plus a separate small thumbnail
  blob, stored whole. Lists and search results load only thumbnails; the full image loads when a
  profile is opened (implemented in the schema and checks; no client yet).
- Location is `place`, a city or district name, never coordinates
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).
- The profile lists the owner's device keys, so a sender can reach every device's mailbox before
  any MLS group exists; anyone can see how many devices a user has.
- The profile carries a proof of work bound to the owner's key (planned,
  [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)): fields `pow_nonce` and
  `pow_bits`, valid when Argon2id(salt `dyapp-profile-pow`, password signer key ‖ nonce, 8 MiB,
  1 pass, 1 lane) starts with `pow_bits` zero bits. It does not depend on the profile's content,
  so it is computed once, about 30 s at 11 bits on a phone. The client computes it in the
  background, keeps the last tried nonce to resume after the app is suspended, and publishes to
  search only when it is done; messaging works before that. A profile without a valid proof
  counts as 0 bits: store nodes keep it, gossip does not relay it, search does not index it.
- Deletion is a signed tombstone kept for the TTL
  ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

### Search

- **Discovery.** Search nodes find each other and clients find them through their own Kademlia,
  `/dyapp/search-kad`: a client looks up the nodes closest to a random key.
- **Dissemination.** Clients do not run gossipsub. On every change the owner's client sends its
  `SignedRecord` to 2 search nodes with `/dyapp/search` `publish`, and once a day a signed
  `heartbeat` (key, profile version, date). The entry node checks the signature, the proof of
  work, the version and the per-key limit, stores the record and publishes it to the gossipsub
  topic `/dyapp/profiles/<n>`, n = H(key) mod 16. The mesh holds search nodes only, not store
  nodes or phones. Gossipsub uses its default degree (D = 6), a message id of
  H(key‖version‖kind) and manual validation before relaying; a bad message is a strike in the
  node's own ban score, gossipsub scoring is not used. The largest message is
  `search.max_profile_bytes` plus overhead.
- **Per-key limits.** At most 1 profile update per 10 min and 2 heartbeats per day per key. The
  entry node answers an excess publish `RATE_LIMITED` with `retry_after` in seconds and never
  drops it silently; gossip relays drop extras silently. The client remembers when it last
  published to search. Within the 10 min it holds the latest profile, merges further edits into
  it, sends it when the window ends (or on the next app start if the app was closed) and tells
  the user "changes appear in search in N min". The profile in the store DHT is updated at once,
  without this limit.
- **Proof of work.** Gossip relays and search nodes index and relay only profiles with enough
  proof of work (`search.min_profile_pow_bits`, default 11, at most 20). A node caches verified
  keys, so Argon2id runs once per key, and a bad proof is a strike against the peer that sent it.
- **Index.** A search node subscribes to all 16 topics by default, or to the part listed in
  `search.topics`, and then holds a uniform sample. The index is its own disposable file,
  `profiles-idx.db`, built from `profiles.db`: typed columns for every `Profile` field except
  photos, plus key, version, heartbeat date and a random u64 `r`, and a table of (key, interest)
  pairs. `r` is drawn again for every new version. Indexes are (country, place, r), (country, r)
  and (r). A profile larger than `search.max_profile_bytes` (default 64 KiB, at most 1 MiB,
  reported in `info`) is not indexed; raising the limit later brings such profiles back through
  heartbeat → `NOT_FOUND` → republish and through inventory. Storage is bounded by
  `search.max_bytes`; a profile leaves the index on a tombstone, when no heartbeat came within the
  retention TTL, or under `search.max_bytes` pressure, oldest heartbeat first. A new indexed field
  bumps the index version; the node builds the new index next to the old one, keeps answering from
  the old one and switches when the new one is ready.
- **Catch-up.** At start and then hourly a node exchanges (key, version) lists per topic, in
  batches of up to 10 000, with one random search node over `/dyapp/inventory`, out of its repair
  budget.
- **Conditions.** A condition is (`Profile` field number, op, values), at most 16 per query and
  one per field. Ops: `eq`; `in` with up to 8 values (for interests: any of them); `range`,
  inclusive, for numbers. Values are int64 or strings; strings match exactly. Income "at least X"
  is `income_to ≥ X`. `country` is optional. A node skips a field or op it does not know and does
  not list it as applied, so the client filters the rest; a value of the wrong type is
  `INVALID`.
- **Queries.** A query is unsigned. The node draws a random `s` and runs
  `WHERE conditions AND r ≥ s ORDER BY r LIMIT n`, wrapping around to the start if fewer rows
  come back, and scans at most `search.max_scan_rows` (10 000) rows, otherwise it sets `partial`.
  The reply holds (identity key, version) pairs, 40 bytes each, 50 by default and at most 200,
  with the conditions applied, `partial` and the index version. A repeated query gives a new
  sample; there is no pagination. The client fetches the records it does not have in its cache
  with `get(keys)`, up to 50 keys, from the same node; the reply is cut at 1 MiB and the client
  asks again for the rest. A `query` costs 1 unit and a `get` the bytes of its reply, limited per
  peer ID and IP group; scraping public profiles is accepted.

### Mailboxes

- Every device is a separate MLS member and has its own mailbox on its store replicas. The sender
  writes a message to the mailbox of every recipient device, so one device's ack never removes
  another device's copy.
- An online device keeps a connection to one replica node and gets new messages pushed at once.
  Clients do not publish their addresses in the DHT.
- The device acknowledges; the node forwards the ack to the other replicas, which drop the
  message, and the device drops duplicates by message id. The sender keeps a retry queue and
  learns of delivery from a receipt the recipient sends as an ordinary encrypted message to the
  sender's mailboxes; nodes never link the two.
- Reading and deleting a mailbox needs a signature over a nonce the node issued for this libp2p
  connection, so a captured request cannot be replayed. Other requests are idempotent.
- Chat attachments are encrypted media blobs. The sender attaches them under the SHA-256 of a
  random release secret and sends the secret inside the message; the recipient releases them
  after downloading, and unreleased ones expire after an operator-set retention.

### Storage on a node

- One SQLite file per data type ([ADR 0015](../decisions/0015-sqlite-node-storage.md)): `profiles.db`, `profiles-idx.db` (disposable, rebuilt),
  `messages.db` (messages and signals), `deny.db` for the operator's deny list. No transaction spans two stores.
- Media blobs are files, never database rows: `<media dir>/aa/bb/<hash>`, written to a temporary
  file, fsync'd and renamed; the name is the SHA-256 of the data. Implemented, without an fsync
  of the directory ([Storage](#storage)).
- No routine full `VACUUM`: stores use `auto_vacuum = INCREMENTAL` with `incremental_vacuum(N)`,
  a bounded WAL (`journal_size_limit`, regular checkpoints) and `PRAGMA optimize`, on one
  schedule for all stores with a page budget per run (implemented, [Storage](#storage)). There
  is no maintenance window: a node serves the whole world and has no quiet hours.
- I/O budget: the shared `RebuildBudget` (`rust/bootstrap/src/maintenance.rs`) lets background
  rebuild workers write a batch then await a reserved time slot outside the store lock, at most
  `maintenance.io_mb_per_s` (default 16 MiB/s, must be positive). Concurrent rebuilds share the
  same allowance. Each maintenance run logs file and percent complete; a finished build is
  reported once. Format rebuild workers remain planned. Incremental vacuum keeps its page
  budget and network repair keeps its traffic cap.
- Format versions (planned): each file holds its format in `PRAGMA user_version`, one number
  that only grows; there is no downgrade, a fix ships as a higher version. On open the node runs
  the numbered steps from the file's version to its own, in order. A cheap step (a new column
  with a default, an index, a table) runs in place. A step that rewrites rows builds the new file
  next to the old one (for example `profiles-v2.db`) from the stored signed records, in the
  background under the I/O budget, while the old file answers and takes writes; the builder then
  copies the rows written since it started (by rowid or `seq`), switches under a short lock and
  deletes the old file. Deletes during the build (acks, expiry, eviction) go to both files, so
  nothing deleted comes back. The build starts only with the old file's size plus `min_free_mb` free,
  otherwise it waits and logs why. A binary that finds a file newer than it knows refuses to start
  and names the file. The first user is `profiles-idx.db`, rebuilt on an index version bump.
  Today the only step is the `last_seen` column of `profiles`, added in place on open.
- Activity: one UPSERT of a user's last-active date per day.

### Limits and abuse

Record size limits are protocol constants, the same on every node: a profile up to 1 MiB, a
message up to 100 KiB, a signal up to 1 KiB, a media blob up to 6 MiB (planned, today 1 MiB).
The client cannot pick the nodes that hold a replica, so a node with a lower limit would leave a
hole and a higher one would hold records no other replica takes; a larger limit needs a new
protocol version. `info` reports them for nodes of different versions. Capacities are space,
not compatibility: a mailbox up to 10 MiB per device, signals up to `limits.signal_max_kb`,
media up to `limits.media_per_owner_mb` per user, each counted by the node over the data it
holds. Write quotas apply per sender key and per IP group; the prefix length (for example /24
or /48) is the operator's choice.
Resource guards cap disk per store and for media (with a free-space reserve), traffic (request rates and
an optional byte rate; near it the node sheds media first, then search, the mailbox last) and
memory (connections, streams, request size). A full store answers "full" so the client tries
another replica.
Signals (planned): a like, a view or any other signal is an ordinary end-to-end encrypted
mailbox envelope with `class = signal`, at most 1 KiB, so a node cannot tell a like from a view.
Signals have their own quota per mailbox, `limits.signal_max_kb` (default 2048, about 8 000
signals), apart from the 10 MB for messages: at the quota the node deletes the mailbox's oldest
signals instead of answering "full", so the likes of a popular profile never block its messages.
The client sends at most one view per profile a day. Implemented: the disk, traffic and
connection guards in [Resource guards](#resource-guards) and the quotas and peer bans in
[Rate Limiting](#rate-limiting).

The operator may refuse service to any user through a deny list (implemented, see
[Deny list](#deny-list)). Nodes can exchange signed lists, with separate switches to share and
to accept, both off by default; a node only stores received lists and acts on none of them.
Illegal media are removed by listing the blob hash; erasure-coded shards of a listed blob are not
mapped yet.

### Operating a node

- Runs as an unprivileged system user and refuses to start as root; default port 7070, every
  path must be writable by that user (implemented in `dyappd`). The Debian 12 package adds a
  hardened systemd unit and the `dyappd` system user
  ([deployment](../operations/deployment.md#debian-12-package)).
- A TOML config sets addresses, paths per store, roles, limits and TTL; environment variables
  override it and unknown keys are ignored, so a rolled-back node still starts. Invalid config
  fails at startup ([Configuration](#configuration)).
- The node key is a libp2p key file (mode 0600) in the data directory, made once by
  `dyappd keygen` with the node-ID proof of work; the node does not start without it.
  A new key is a new node: the node refuses a key that does not match the stored data, and the
  operator deletes the data. On a leak or a move the operator creates a new key.
- Settings live in the config file and the systemd unit and need a restart. The `dyappd` binary
  also carries the admin subcommands: `deny add|remove|list` edits `deny.db`, which the running
  node applies within 10 seconds and logs ([Deny list](#deny-list)), `deny received` prints the
  lists other nodes shared, and `status` prints what
  the stores hold, read-only. There is no TUI and no admin HTTP endpoint; metrics go to the log.

## Data Model

### Mailbox envelope

A message for one device is a `SignedRecord` signed by the sender's device key over
`"dyapp/envelope/v1\0" || payload`, where the payload is an `Envelope`
([schema](protobuf-schema.md#node-protocol)):

```
Envelope { id: 16 random bytes, mailbox: SHA-256(recipient device key), ciphertext }
```

The node stores the signed record as received, once per (mailbox, id), and never learns the
sender's identity: the sender key only proves someone signed it; puts are limited per sender key
([Rate Limiting](#rate-limiting)).

**Lifecycle:** put → stored until the device acks it or `limits.message_ttl_hours` passes
(`dyappd` deletes expired envelopes every hour) → fetched oldest first.

Planned: `Envelope.class`, `message` (default) or `signal`, the only field that tells a node a
signal from a message; signals count against `limits.signal_max_kb`, not the mailbox's 10 MiB
([Limits and abuse](#limits-and-abuse)).

### Signed profile

Source: `rust/identity` (keys, `SignedRecord`) and `rust/profile` (`Profile`, checks).

```rust
pub struct SignedRecord {          // protobuf, sent and stored as is
    public_key: Vec<u8>,           // owner's Ed25519 identity key (32 bytes)
    payload: Vec<u8>,              // protobuf-encoded Profile
    signature: Vec<u8>,            // Ed25519 over "dyapp/profile/v1\0" || payload
}
```

The peer ID is the hex SHA-256 of `public_key`; the server derives it, the client
never sends it. `Profile` holds `version`, `deleted` and the public fields (age,
country, place (city or district, no coordinates), income range, kids, goals, interests, …); every field is
optional and public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).

`/dyapp/profile` `publish` stores a record only if:

- the signature verifies against `public_key`;
- the payload is at most 1 MiB and decodes as a `Profile` (media are separate blobs on `/dyapp/media`);
- `version` ≥ 1 and greater than the stored version for that peer ID (otherwise `STALE`);
- content is sane: country is an ISO 3166-1 alpha-2 code, income range not reversed,
  place at most 1024 characters without control characters, at most 16 `photos`, each a
  32-byte thumbnail hash and 1–64 full-image blob hashes, a tombstone (`deleted = true`) carries no other field.

Deletion publishes a tombstone with a higher version. The server keeps the tombstone
so an older version cannot be re-imported; `get` returns it so peers learn of the deletion.
A profile, tombstone included, is deleted `limits.profile_ttl_days` (default 30) after its
owner was last seen: a publish of a newer version or an identity-signed heartbeat whose time
is within 10 minutes of the node clock and newer than the stored `last_seen`. Heartbeats advance
`last_seen` to their signed time, so repeating one does not extend retention. Stale republishes,
media requests and mailbox requests do not refresh profile liveness. The client is
meant to send a heartbeat to the profile's replicas when the app opens (planned; no client yet).
Full field table: [Privacy & Metadata Visibility](../security/privacy.md).

## Served protocol

Source: `rust/bootstrap/src/service.rs` (request handling), `node.rs` (the libp2p
loop), `rust/p2p-net` (`ProtoCodec`, protocol IDs), `storage.rs` (SQLite), `media.rs` (blobs).
`dyappd` serves these libp2p request-response protocols over TCP and QUIC, one protobuf request and one reply
per stream ([schema](protobuf-schema.md#node-protocol)). Each request and reply is capped at
1 MiB + 64 KiB, except `/dyapp/media`, which allows 6 MiB + 64 KiB. These are wire limits;
the payload and storage limits below are checked separately:

| Protocol | Request | Reply |
|----------|---------|-------|
| `/dyapp/node` | `info` | `STATUS_OK`, roles (`ROLE_STORE`, `ROLE_MEDIA`), `max_media_bytes` (1 MiB, media role only), `max_profile_bytes` (1 MiB), `max_message_bytes` (100 KiB), `max_mailbox_bytes` (10 MiB), `retention_seconds` (`limits.message_ttl_hours`); a node without the store role serves no protocol |
| `/dyapp/node` | `turn` | `OK` with coturn credentials and URLs ([TURN](#turn-credentials)); `RATE_LIMITED`; `REFUSED` a denied peer; `UNSUPPORTED` without the turn role and on older nodes |
| `/dyapp/node` | `deny_list` | `OK` with the signed list when `network.share_deny_list` is on, else `NOT_FOUND` ([exchange](#deny-list-exchange)) |
| `/dyapp/profile` | `publish(SignedRecord)` | `OK`; `STALE` with the stored record when the version is not newer; `DENIED` bad signature; `INVALID` bad key or content; `TOO_LARGE` payload over 1 MiB |
| `/dyapp/profile` | `get(peer_id)`, 32 raw bytes | `OK` with the record, tombstone included; `NOT_FOUND`; `INVALID` wrong length |
| `/dyapp/profile` | `heartbeat(SignedRecord)`, payload `Heartbeat { time }` signed by the identity key | `OK` advances `last_seen` to the signed time if newer; `NOT_FOUND` no profile, publish it; `DENIED` bad signature; `INVALID` time more than 10 min off; `REFUSED` denied key; older nodes `UNSUPPORTED` |
| `/dyapp/mailbox` | `challenge` | `OK` with a fresh 32-byte nonce for this connection; it replaces the previous one |
| `/dyapp/mailbox` | `put(SignedRecord)`, payload `Envelope` | `OK`, also for a repeated (mailbox, id); `DENIED` bad signature; `INVALID` undecodable, id not 16 or mailbox not 32 bytes; `TOO_LARGE` payload over 100 KiB; `FULL` the mailbox would exceed 10 MiB |
| `/dyapp/mailbox` | `fetch(SignedRecord)`, payload `Fetch` | `OK` with the oldest envelopes (at most `limit`, 100 and 1 MiB per reply) and `more`; `DENIED` |
| `/dyapp/mailbox` | `ack(SignedRecord)`, payload `Ack` | `OK`, the listed ids are deleted, unknown ones ignored, and the ack is forwarded as `replica_ack`; `DENIED`; `INVALID` an id not 16 bytes |
| `/dyapp/mailbox` | `replica_ack(SignedRecord)` | an `ack` another node forwards verbatim: checked like `ack` without the nonce, not forwarded again; same replies |
| `/dyapp/mailbox` | `inventory(Inventory)`: mailbox, at most 10 000 ids (node to node) | `OK` with `missing`, the listed ids the node does not hold; `INVALID` mailbox not 32 bytes, an id not 16 or too many ids; `REFUSED` a denied mailbox; `RATE_LIMITED` from 75 % of the traffic cap |
| `/dyapp/mailbox` | `replica_put(Envelopes)` (node to node) | each envelope stored like a `put`; `OK`, else `DENIED` when any signature is forged, else the first failure |
| `/dyapp/mailbox-push` | `MailboxPush` (node to client) | the envelope just stored, sent once per connection that sent a `fetch` with `watch` |
| `/dyapp/media` | `keep(SignedRecord)`, payload `MediaKeep` | `OK` with `missing`, the listed hashes the node does not hold yet; `STALE` older version or repeated time; the same version and list with a newer signed time refreshes liveness; `DENIED` bad signature; `INVALID` bad version, over 256 hashes, bad hash length or time more than 10 min off |
| `/dyapp/media` | `put(MediaPut)`: owner = SHA-256 of the identity key, data | `OK`, also for a blob already held; `NOT_FOUND` the owner's list lacks SHA-256(data); `TOO_LARGE` over 1 MiB; `FULL` over `limits.media_per_owner_mb` or the media disk guard; `INVALID` owner not 32 bytes |
| `/dyapp/media` | `get(GetMedia)`: hash | `OK` with the blob; `NOT_FOUND`; `INVALID` hash not 32 bytes; planned: `offset` and `len` for a piece of at most 1 MiB with the total size ([Replication](replication.md)) |
| `/dyapp/media` | `attach(SignedRecord)`, payload `MediaAttach` | `OK` with `missing`; `DENIED` bad signature; `FULL` 256 unreleased attachments of the sender; `INVALID` release hash or a blob hash not 32 bytes, no or over 16 hashes, `created` over 10 minutes ahead |
| `/dyapp/media` | `release(MediaRelease)`: secret | `OK` attachments dropped; `NOT_FOUND` none with SHA-256(secret) |

- **Mailbox authorisation.** `fetch` and `ack` act on the mailbox whose address is SHA-256 of the
  signing key, so a device reaches only its own mailbox. They are signed over
  `"dyapp/mailbox-fetch/v1\0"` or `"dyapp/mailbox-ack/v1\0"` and the payload, which carries the
  nonce from the last `challenge` on the same connection. The node takes the nonce on any fetch
  or ack, valid or not: a replay, a nonce from another connection, a request signed for the other
  domain or one without a challenge gets `DENIED`, and the client asks for a new challenge.
  Nonces live in memory and go with the connection or a restart.
- **Push.** A valid `fetch` with `watch` registers its connection for that mailbox until the
  connection closes; every later `put` to the mailbox is pushed there. Replies to a push are ignored.
- **Ack forwarding.** After a valid `ack` the node looks up the closest node of each
  `replica_key(mailbox, i)` and sends it the signed ack as `replica_ack` if the node trusts it
  ([Rate Limiting](#rate-limiting)); replies are ignored.
  Only the closest node per key gets it, and a node that missed it keeps the envelope until its
  TTL. A replayed `replica_ack` can only delete ids the device already acked.
- **Mailbox repair.** A valid `fetch` with `watch` starts a repair of that mailbox, at most once
  an hour per node and only below 75 % of the traffic cap. The node looks up the closest node of
  each `replica_key(mailbox, i)`, skipping keys it is closer to itself, and sends it an
  `inventory` of the ids it holds; the envelopes the answer lists as `missing` follow in one
  `replica_put`, again only to a trusted node. The peer, if one of the first five nodes its own
  routing table holds for a replica key is the requester and it trusts the requester, sends back
  the envelopes the inventory lacks the same way. A batch is cut at 1 MiB, the rest waits for the
  next visit; repair bytes count towards the traffic cap. A repaired envelope gets a fresh TTL on
  its new node. Old nodes answer both requests `UNSUPPORTED`, and repair skips them.
- **Media.** The owner's signed `keep` is the whole list of blobs the node should hold for that
  identity, replaced by a higher `version`; a blob dropped from every owner's list is deleted at
  once. A put needs no signature: the list authorises it, and a replay stores nothing new. A blob
  kept by two owners counts against both quotas and needs only one upload. The media store
  keeps its own `last_seen` per owner, advanced to the signed time of a `keep` or `attach`;
  hourly cleanup deletes an inactive owner's keep after `limits.profile_ttl_days`, together
  with blobs held by no other keep or attachment. Attachments retain their separate expiry.
  At the disk size limit, old attachments then inactive owners' keeps are evicted down to
  95 %; shared blobs stay, and `FULL` is returned only if space is still insufficient.
  `MediaKeep` carries a signed `time`; a `keep` refreshes `last_seen` only if `time` is within
  10 minutes of the node's clock
  and newer than the stored one, so a replayed `keep` extends nothing, while the client resends
  its current `keep` with a fresh `time` when the app opens, uploads what `missing` lists and so
  restores evicted blobs. A deny-listed key cannot refresh its `keep`, so it expires. Media
  requests have their own per-peer limit (`limits.media_requests_per_second`, default 10, no
  strike) besides the shared ones. A node without the media role answers `UNSUPPORTED`. Ranges,
  media replication and repair are planned.
- **Chat attachments.** A signed `attach` lists up to 16 blobs under the SHA-256 of a release
  secret; the blobs are put like listed ones and count against the sender's quota. Anyone holding
  the secret (the recipient, after downloading) sends `release` and the node drops the
  attachment at once; a blob also kept or attached elsewhere stays. An unreleased attachment
  expires `limits.attachment_retention_hours` (default 168) after its `created` time, which may
  be at most 10 minutes ahead of the node's clock; the hourly cleanup deletes it. A sender has at
  most 256 unreleased attachments per node (`FULL` beyond). A replayed `attach` restores the
  attachment until that same expiry. Nodes older than attachments answer both requests
  `UNSUPPORTED`.
- <a id="deny-list"></a>**Deny list.** `<storage.dir>/deny.db` (SQLite, keyed by entry) holds
  libp2p peer IDs, IP groups as the node computes them (for example `203.0.113.0/24` with the
  default prefix) and lowercase hex SHA-256 hashes: of an identity or device key, or of a media
  blob, each with a note and the time it was added. The operator edits it with
  `dyappd deny add <entry> [note]`, `deny remove <entry>` and `deny list`, run as the owner of
  `storage.dir`; `add` refuses anything else and masks an address to its group (`203.0.113.9/24`
  is stored as `203.0.113.0/24`; another prefix length is refused). The running node checks the
  store every 10 seconds and on SIGHUP, reloads it when it changed and logs the entry count and
  how many were added and removed; a read error keeps the old list. The node holds the entries
  in memory. An old `<storage.dir>/deny` text file (one entry per line, `#` comments) is imported
  once at start and renamed to `deny.imported`; lines that do not parse are logged and skipped.
  A listed peer or IP group gets `REFUSED` on every profile, mailbox and media request; a listed
  key gets `REFUSED` on its profile publish and get, on puts it signs or addressed to its
  mailbox, on its fetch and on its media `keep` and `put`. `REFUSED` is no strike, so the client
  moves to another replica; one node's list removes no one from the network. Acks are still
  served, and a `get` of a listed owner's blob already stored is still answered (blobs are not
  indexed by owner on read); a listed blob hash gets `REFUSED` on `get` and `put`, and its file
  stays until no keep or attachment holds it.
- <a id="deny-list-exchange"></a>**Deny-list exchange.** With `network.share_deny_list` on, a
  `/dyapp/node` `deny_list` request gets the entries (no notes), sorted, cut at 20 000 so they fit
  one message, with the signing time, as a `SignedRecord` signed by the node's Ed25519 libp2p key
  (domain `dyapp/deny-list/v1`); the node signs anew whenever it reloads its list. Off, it answers
  `NOT_FOUND`; old nodes answer `UNSUPPORTED`. With `network.accept_deny_lists` on, every
  maintenance run asks each connected routed peer and stores a list whose key is the peer's own
  in the `received` table of `deny.db`, replacing an older one of the same node; lists of the
  1000 nodes heard from last are kept. The node takes no action on them: `dyappd deny received`
  shows them to the operator. A unit test checks the signature, the signer and the replacement.
- <a id="turn-credentials"></a>**TURN credentials.** A node with the `turn` role (it needs
  `store`) hands out credentials for the coturn relay its operator runs, in coturn's
  `use-auth-secret` scheme: username `<expires>:<peer id>`, password base64(HMAC-SHA1(`turn.secret`,
  username)), valid `turn.credential_minutes` (default 60), with the `turn:`/`turns:` URLs from
  `turn.urls`. The request is rate-limited like a profile get. Once its routing table has a peer the
  node announces itself as a Kademlia provider of `/dyapp/turn`; kad republishes the record every
  12 hours. Clients find relays with `get_providers` on that key; using them for call ICE is
  planned.
- **Profile and mailbox requests are rate-limited** per remote libp2p peer ID and IP group, puts
  also per sender key (`RATE_LIMITED`); a banned peer is disconnected ([Rate Limiting](#rate-limiting)).
  `info` is not limited. A node without the store role answers `UNSUPPORTED` on both.
- **Storage errors drop the request:** the client sees the stream close and tries another node.
- **Requests run on the swarm loop.** SQLite calls block it; moving them to a blocking pool is
  planned once request latency shows.
- An unknown `oneof` variant (decoded as empty) gets `UNSUPPORTED`.

### Trusted vs untrusted fields

Profile content is signed, so a client re-verifies a fetched record (`dyapp_profile::verify`) and
then trusts it as the owner's own claim, not as fact:

| Field | Who sets it | What the node checks |
|-------|-------------|----------------------|
| profile peer ID | node | derived from the signing key |
| profile fields | owner | signature, version order, format checks above |
| `age` | owner | nothing beyond `u32`; no minimum, so an age below 30 or 18 is stored |
| envelope `mailbox`, `id` | sender | lengths only; any key may put into any mailbox |
| envelope `ciphertext` | sender | size only; the recipient checks the sender inside it |

- **Profiles are owner-only.** Only the holder of the identity key can publish, replace or delete
  (tombstone) a profile. A replayed old version is rejected.
- **Age.** Age is self-declared; the client refuses users under 18 in its UI, and 30+ is only the
  target audience ([ADR 0012](../decisions/0012-private-p2p-interactions.md)). The node checks
  nothing beyond the signature: it would accept `17`.

### Test client

`test-peer` is a libp2p client for scripts (`rust/bootstrap/src/bin/test-peer.rs`); each command
prints one JSON object:

```
test-peer sign-profile                      → {"peer_id", "record"}   (hex protobuf)
test-peer info    /ip4/127.0.0.1/tcp/7070   → {"status", "peer_id", "roles", "max_profile_bytes"}
test-peer publish <multiaddr> <record hex>  → {"status"} (+ "record" when stale)
test-peer get     <multiaddr> <peer_id hex> → {"status", "record"}
test-peer device-key                        → {"secret", "mailbox"}
test-peer put     <multiaddr> <mailbox hex> <ciphertext hex> → {"status", "id"}
test-peer put-replicas <multiaddr> <mailbox hex> → {"id", "holders"}
test-peer watch   <multiaddr> <secret hex>  → {"status", "id", "pushed"}
test-peer fetch   <multiaddr> <secret hex>  → {"status", "ids", "more"}
test-peer ack     <multiaddr> <secret hex> <id hex>... → {"status"}
test-peer closest <multiaddr> <key hex>     → {"peers"}   (DHT lookup through the node)
test-peer replicate <multiaddr> <peer_id hex> <record hex> → {"holders"}
test-peer flood   <multiaddr> <n>           → {"<status>": count, "failed": count}
test-peer media   <multiaddr> <data hex>    → {"keep", "put", "get"}
test-peer media-owned <multiaddr> <data hex> → media statuses + details {owner, hash, keep_record}
test-peer media-keep <multiaddr> <record hex> → {"status", "data"}
test-peer media-get <multiaddr> <hash hex>  → {"status", "data"}
test-peer turn    <multiaddr>               → {"status", "username", "password", "urls", "expires"}
test-peer relays  <multiaddr>               → {"providers"}   (TURN relays in the DHT)
```

`put` signs with a fresh sender key; `fetch` and `ack` get a challenge and use it on one
connection. `closest` lists the nodes that answered, closest first. `replicate` publishes each of
the `REPLICAS` replicas of a profile to the node closest to `replica_key(peer_id, i)`; a second
replica on the same node answers `STALE`, which counts as stored. `put-replicas` puts one
envelope to the node closest to each `replica_key(mailbox, i)`. `watch` sends a watching `fetch`,
puts an envelope to its own mailbox on another connection and prints the ids pushed back. `flood`
sends `n` profile gets at once on one connection. `media` has a fresh owner keep the blob's hash,
put the blob and get it back on one connection and prints each status; `"get"` is `"CHANGED"` if
the blob came back different.

It has no DNS transport: pass `/ip4/` or `/ip6/` addresses, without `/p2p/`.

## Erasure coding (Reed-Solomon)

Profiles and mailboxes are replicated whole ([Replication and repair](#replication-and-repair));
erasure coding is meant only for large media. **Status:** `rust/bootstrap/src/replication.rs`
only encodes a byte buffer into shards and decodes it back, in one process. Nothing in
`service.rs` or `storage.rs` calls it and the node config has no setting for it. Placing shards on
nodes, sending them and verifying them are planned with media replication.

A Reed-Solomon code with `d` data and `p` parity shards survives the loss of
any `p` shards and needs any `d` of the `d + p` shards to rebuild. With one
shard per node, the tolerated failures are `p` nodes, **not** a fixed 50%.

Mapping in `Replication::new_with_replication_factor`:

| RF | Data | Parity | Total shards | Min surviving | Max lost | Tolerated share | Storage overhead |
|----|------|--------|--------------|---------------|----------|-----------------|------------------|
| 1 | 1 | 0 | 1 | 1 | 0 | 0% | 1× |
| 2 | 1 | 1 | 2 | 1 | 1 | 1/2 (50%) | 2× (equivalent to mirroring) |
| 3 | 2 | 1 | 3 | 2 | 1 | 1/3 (~33%) | 1.5× |
| 4 | 3 | 1 | 4 | 3 | 1 | 1/4 (25%) | ~1.33× |

The planned code for media above the node's threshold is K = ⌈size / 1 MiB⌉ data shards (at
most 6, for the 6 MiB blob limit) and M = 4 parity, surviving the loss of any 4
([Replication design](replication.md), [ADR 0009](../decisions/0009-message-delivery-and-storage.md));
messages and profiles are replicated whole. This mapping does not produce it.

**Example: RS(2,1) on 3 nodes**
```
Original record (100 bytes)
  ↓ encode RS(2,1)
Shard 0: 50 bytes (data)    → Node A
Shard 1: 50 bytes (data)    → Node B
Shard 2: 50 bytes (parity)  → Node C
  ↓
Node A fails → rebuild from shards 1 + 2 (B + C)
Nodes A and B fail → only 1 shard left, record lost
```

Codec unit tests show that a buffer can be rebuilt with one shard missing; no node stores shards
yet.

## Rate Limiting

`rate_limit.rs` keeps one `governor` token bucket per key with
`Quota::per_second(n)`; `n` is passed by the host program to
`PeerRateLimiter::new(n)` (`dyappd` passes `limits.requests_per_second`, default 100).
The bucket allows a burst of `n` requests and then refills at `n`/second; an
over-limit request gets `STATUS_RATE_LIMITED`.

Scope and limits:

- Every `/dyapp/profile` and `/dyapp/mailbox` request counts against two buckets: the remote
  libp2p peer ID (`requests_per_second`) and its IP group (`ip_group_requests_per_second`,
  default 1000), the connection's remote IP masked to `ipv4_prefix` / `ipv6_prefix` bits
  (defaults /24 and /48). `/dyapp/node` `info` is not limited.
- A mailbox `put` also counts against the envelope's sender key (`sender_puts_per_second`,
  default 10), after the signature is checked.
- libp2p keys and sender keys are free to generate; the IP-group bucket bounds what a client
  gains by rotating them. A relayed connection gets the relay's group. There is no Sybil
  resistance beyond that.
- Once a map holds 10 000 buckets, buckets idle for a second (refilled, so dropping them changes
  nothing) are removed before a new one is added.

**Local peer reputation.** Each refusal by the peer or IP-group bucket and each `DENIED` reply
(a bad signature, a replayed or missing mailbox nonce) is a strike against the peer ID. At
`strikes_to_ban` strikes (default 100), each within `ban_minutes` (default 10) of the previous one,
the peer is banned: the node closes its connections, closes any new ones, and answers
`RATE_LIMITED` to requests still in flight, until `ban_minutes` pass without a strike. Scores live
in memory, go with a restart and are never shared with other nodes
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)).

**Storage trust.** Replicas (forwarded acks, repair) go only to trusted peers. A peer is trusted
once the node first connected to it at least `network.storage_trust_minutes` ago (default 60; 0
trusts at once) and while its score is not negative. The score gains 1 per answer to the node's
mailbox requests and per signed `replica_put` or `replica_ack` accepted from the peer, and loses 1
per mailbox request of the node that failed (an old node lacking the protocol does not count). A
peer that leaves both the routing table and the node's connections is forgotten. Like the ban
score it lives in memory and is never shared, so after a restart no peer gets replicas for the
delay; envelopes wait on their own node until their TTL. A unit test checks it.

## Resource guards

From `limits` in the config ([Configuration](#configuration)); each guard logs a warning when it
starts refusing and a line when it clears, not one per request:

- **Disk**: before a profile publish or mailbox put the node reads the store's live data
  (pages in use) and the free space of its file system (`statvfs`). At `profiles_max_mb` /
  `messages_max_mb` (defaults 1024 / 4096) or at `min_free_mb` free (default 512) the write gets
  `FULL`; reads, acks and expiry go on, so space comes back. A store at its size limit first
  evicts its oldest records down to 95 % of the limit, at most 64 × 64 rows per write, and
  refuses only if that is not enough: profiles in publish order (a republish makes a profile
  young), envelopes in arrival order ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).
  The free-space floor is not cleared by eviction, since freed pages stay in the file until
  maintenance. Media puts get `FULL` at `media_max_mb` of distinct blobs (default 10240) or at
  `min_free_mb` free on the media file system. At `media_max_mb` the node first evicts down to
  95 % of it: unreleased chat attachments by oldest `created`, then the `keep` of owners seen
  longest ago, and answers `FULL` only if that is not enough; a blob still held by another
  `keep` or attachment stays. Eviction is bounded to 4096 lists per category per put;
  assembled-blob cache eviction is planned with ranged media reads.
- **Traffic**: node protocol bytes in and out (encoded requests and replies, not transport
  overhead) are counted per second. With `bytes_per_second` set (default 0, no limit), media
  requests and repair get `RATE_LIMITED` from 75 % of it within the current second, profile
  requests from 90 % and mailbox requests at 100 %; `info` is always served. Search, once
  served, is shed before profiles. There is no monthly cap: a node is not told its billing
  period, and the byte rate bounds a month too.
- **Guard metrics**: a guard that starts or stops refusing logs once; each maintenance run logs
  per guard how many requests it refused since the last run (the byte rate, which flips every
  second under load, only here).
- **Connections and memory**: libp2p connection limits — `max_connections` established and
  pending incoming (default 1000), `max_connections_per_peer` (default 4) — and `max_streams`
  concurrent streams per connection and protocol (default 16); frames are capped at
  1 MiB + 64 KiB, or 6 MiB + 64 KiB on `/dyapp/media`.
  `max_memory_mb` (default 768, the packaged unit's `MemoryHigh`; 0 = no limit) bounds the
  process's physical memory, sampled at most every 100 ms. From 80 % of it media, repair,
  profile and mailbox requests get `RATE_LIMITED` (no strike); at 100 % every request is
  dropped unanswered, which resets its stream so the client tries another node, and libp2p
  `memory-connection-limits` refuses new connections. Established connections stay. Both memory
  guards log when they trip and clear and count refusals per maintenance run; refused
  connections are counted and logged at each maintenance run.

## Storage

One SQLite file per data type in the storage directory, WAL journal,
`auto_vacuum = INCREMENTAL`, `journal_size_limit` 64 MiB:

```
profiles.db  profiles(peer_id PK, record BLOB, live)
             record: SignedRecord (protobuf), latest version or tombstone (live = 0)
messages.db  envelopes(seq PK, mailbox BLOB, id BLOB, record BLOB, size, expires_at,
                       UNIQUE (mailbox, id))
             record: the signed envelope as received; indexes on (mailbox, seq), expires_at
media.db     owners(owner PK, version)  blobs(owner, hash, size, PK (owner, hash))
             owner: hex SHA-256 of the identity key; size NULL = listed, not yet put
data/aa/bb/<hex hash>  the blob
```

A `messages` table left by an older version is not read; it can be dropped by hand.
`put_envelope` sums the mailbox's `size` on each put under the store lock.

Each file has one connection behind a mutex. There is no schema version and no
format change path yet; the target builds a new format next to the old one
([Principles](#principles)).

**Compaction:** `BootstrapStore::cleanup_expired` deletes expired envelopes; `dyappd` calls it every hour, then `expire_profiles` deletes profiles and tombstones whose owner was last
seen more than `limits.profile_ttl_days` ago (`last_seen` column, added on upgrade with the
upgrade time for existing rows); under a full quota `evict_profiles` and `evict_envelopes` delete the oldest rows by rowid ([resource guards](#resource-guards)). `INSERT OR REPLACE` gives a republished profile a new, highest rowid, so rowid order is publish order.

**Maintenance:** every `maintenance.interval_minutes` (default 60) `dyappd` calls
`BootstrapStore::maintain`, which, one store at a time under its lock, frees at most
`maintenance.vacuum_pages` free pages (default 2048 = 8 MiB) with `incremental_vacuum`,
checkpoints the WAL with `TRUNCATE` and runs `PRAGMA optimize`, then logs the free pages left.
There is no full `VACUUM`: a large freelist shrinks over several runs, and requests to that store
wait for one run only. Not measured yet: fragmentation under load, and whether time-bucketed
message files beat deletes.

`put_profile` holds a lock across read-compare-write, so two concurrent uploads for one peer cannot both win.

## Configuration

`NodeConfig` (`rust/bootstrap/src/config.rs`) is read by `dyappd`: defaults, then a TOML
file (`--config`), then `DYAPPD__<SECTION>__<KEY>` variables, then `--<section>.<key> <value>`
options (unknown ones refused; `--help` prints them from `rust/bootstrap/dyappd.toml`, which a
test keeps equal to the defaults). Every field has a default and
unknown keys are logged and ignored, so configs work across upgrades and rollbacks. Keys:
`listen`, `external`, `seeds` ([joining](p2p-networking.md)), `roles`, `storage.dir` (every store
in it, media blobs in `<dir>/data`; the removed `storage.{profiles,messages,media}` are refused),
`limits.{message_ttl_hours,requests_per_second,media_requests_per_second,media_per_owner_mb,attachment_retention_hours}`,
`limits.{profiles_max_mb,messages_max_mb,media_max_mb,min_free_mb,bytes_per_second}`,
`limits.{max_connections,max_connections_per_peer,max_streams,max_memory_mb}`
([Resource guards](#resource-guards)),
`limits.{ip_group_requests_per_second,ipv4_prefix,ipv6_prefix,sender_puts_per_second,strikes_to_ban,ban_minutes}`
([Rate Limiting](#rate-limiting)), `maintenance.{interval_minutes,vacuum_pages,io_mb_per_s}`,
`network.{id_pow_bits,distinct_outbound_groups}` ([P2P networking](p2p-networking.md)),
`network.storage_trust_minutes` ([Rate Limiting](#rate-limiting)),
`network.{share_deny_list,accept_deny_lists}` ([Deny-list exchange](#deny-list-exchange)),
`turn.{urls,secret,credential_minutes}` ([TURN](#turn-credentials)); the
example and startup checks are in [Deployment](../operations/deployment.md#dyappd). `dyappd`
starts the libp2p node ([P2P networking](p2p-networking.md)) in `Mode::Auto` with the stores open
and serves the protocols in [Served protocol](#served-protocol). Only the `store`, `media` and
`turn` roles are accepted, `media` and `turn` only together with `store`. On start it dials its
anchors, then fills the routing table from the peer cache; the seeds follow
([P2P networking](p2p-networking.md#current-code)).

Planned: the remaining resource guards, store retention.

## Deployment Model

Every node is independent: there is no cluster, leader or shared storage. Nodes find each other
through the DHT ([P2P networking](p2p-networking.md)), and a client writes each profile and
mailbox replica to the node closest to its replica key, so losing a node loses only the data whose
other replicas are gone too. Mailboxes are repaired when their device watches; profiles and media
wait for the owner's client to republish. A single node (development, or a network of one) keeps
the only copy. The network test runs three nodes and stops one.

## Security

### What bootstrap CAN do

✅ See peer IDs, client IP addresses, who messages whom and when
✅ See every profile field (public by design)
✅ See message content too, until client crypto exists
✅ Rate-limit requests per libp2p peer ID and IP group and ban misbehaving peers locally (no Sybil resistance, see [Rate Limiting](#rate-limiting))

### What bootstrap could not do once client crypto exists (target)

Today the operator can read every stored message (no client encrypts yet). Other clients cannot
read or delete a mailbox: that needs the device key. A node cannot forge an envelope or a
profile, since both are signed, but it can withhold, drop or delay them, or serve an older
signed profile version.


❌ Read encrypted messages
❌ Decrypt anything (no session keys)
❌ See call content (no signaling endpoint exists; media goes peer to peer)

It could still link peer IDs to IPs and public profile fields; deanonymization is not
prevented. See [Privacy & Metadata Visibility](../security/privacy.md).

## Monitoring & Metrics

There is no health endpoint: `/dyapp/node` `info` answering is the liveness check. Metrics are
log lines without peer or key identifiers. Each maintenance run logs `node status`: connected and
known peers, requests answered and failed with a node error since the last run, stored bytes of
profiles, messages and media; plus refusals per
guard and refused connections ([Resource guards](#resource-guards)). `dyappd status` prints what
the stores hold; there are no HTTP endpoints. See the
[Deployment Guide](../operations/deployment.md#monitoring).

## Testing

Unit test coverage:
- Envelopes once per (mailbox, id), mailbox full, fetch limits and `more`, ack, expiry (`storage.rs`)
- Signed profile: version order, forged payload, tombstone hides and blocks older versions (`storage.rs`)
- Rate limiting (single key, separate keys)
- Replication codec encode/decode with a missing shard (in-process; no nodes involved)
- Store health check (`storage.rs`)
- Profile and node requests through `Service` (`service.rs`): publish, get, stale with the
  stored record, forged, wrong-length ID, rate limit, `info`
- Mailbox through `Service`: put, forged and wrong-domain put, bad id, too large; fetch without
  a challenge, with another connection's nonce, signed as an ack, replayed; another key reads
  and acks only its own mailbox; ack and its replay
- `ProtoCodec` round trip over TCP between two swarms (`rust/p2p-net`)

Protocol tests (`rust/bootstrap/tests/protocol.rs`): `node::run` on loopback TCP with libp2p
clients:
- Every request type: `info`, publish and get, mailbox put, challenge, fetch and ack
- A challenge from one connection is `DENIED` on another and still valid on its own
- Bad input fails only its request: undecodable protobuf and a request over its protocol's
  wire limit get the
  stream closed without a reply (a client reads `STATUS_UNSPECIFIED`), an unknown or empty
  variant gets `UNSUPPORTED`, and the node then answers the next client
- Rate limit over the wire: `RATE_LIMITED` past the burst, `info` still answered

Integration tests (`tests/integration_tests.rs`, in-process, no network):
- Replication codec with one lost shard (`test_replication_fault_tolerance`)


The network test (`scripts/network-test.py`) runs three `dyappd` instances, the first with
roles `store` and `media`, and drives them with `test-peer`: `info` over TCP and QUIC, publish
over TCP and get over QUIC with identical bytes, `STALE` on replay; a mailbox put over TCP and
fetch over QUIC, a stranger's key reads nothing, ack empties it, a watching fetch gets a push; a
media keep, put and get on the media node and `UNSUPPORTED` from the others; that the nodes do
not share storage; DHT routing, profile and mailbox replicas, ack forwarding, repair and the
rate limit. CI runs it with `--local`, which also stops a node and checks replicas skip it.
The local run ages media fixture rows, refreshes one keep through its signed protocol request,
then restarts the media node: startup cleanup removes the expired owner's blob while the
refreshed owner's blob remains readable. The same local suite runs inside the prepared dev
Docker image, with temporary storage and reports under `/tmp/ai`.

## Limitations & Future

**Current limitations:**
- Replicas are written by the client, and no client does the replica lookup yet; nodes forward acks and repair mailbox gaps only when a device watches its mailbox
- Media are not replicated and the Reed-Solomon codec is not wired into storage or the protocol
- No audit logging

Planned changes: see [Target Design](#target-design--planned).
