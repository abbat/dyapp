# Bootstrap Server Architecture (Distributed Relay)

## Overview

A bootstrap node (`dyapp-node`) serves owner-signed profiles and per-device mailboxes over
libp2p ([Served protocol](#served-protocol)), pushes new envelopes to watching devices and
repairs mailbox replicas node to node; profile search and signaling are planned.

> ⚠️ The server stores whatever bytes clients send as envelope ciphertext; no client encrypts
> yet. Profile writes need the owner's signature; mailbox reads and deletes need the device's.
> See [Encryption & Security Status](../security/encryption.md).

**Key principles:**
- **Replication (target)**: messages and profiles replicated whole to 5 points, Reed-Solomon K=6/M=4 only for large media [ADR 0009](../decisions/0009-message-delivery-and-storage.md); today only a local encode/decode codec exists and each server is a single node (see [Replication](#replication-strategy-reed-solomon))
- **Mailboxes**: one per device, read and emptied only with the device key's signature; envelopes expire after a TTL (default 24h, deleted hourly; see [Privacy](../security/privacy.md#retention))
- **Profile storage**: public profiles signed by the owner's identity key; the highest version wins and deletion is a signed tombstone ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); no search endpoint yet
- **Rate limiting**: per-peer token bucket on profile and mailbox requests (see [Rate Limiting](#rate-limiting))
- **Encryption (target)**: clients end-to-end encrypt messages and media before upload; profiles are public and signed, not encrypted ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); not implemented
- **Network (target)**: open, anyone may run a node, DHT discovery ([ADR 0007](../decisions/0007-open-bootstrap-network.md)); storage is a cache with an operator-set retention TTL (default 30 days) and eviction by profile activity ([ADR 0009](../decisions/0009-message-delivery-and-storage.md))

## Target Design — planned

`/dyapp/node` `info`, `/dyapp/profile` and `/dyapp/mailbox` on a single node are served today
([Served protocol](#served-protocol)); the rest of this section is planned, and the sections
after it describe today's code.

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
| store | signed profiles, per-device mailboxes, likes, views and other signals |
| media | media blobs and erasure-coded shards, thumbnails |
| search | the search index over profiles collected from the whole network |
| TURN | relays call media |

Every role has its own DHT key space, so replicas for a role are chosen only among the nodes
that run it. A node announces its roles through libp2p identify. A reachable node is a DHT
server; a node behind NAT is a bootstrap only for peers in its local network. Mobile clients run
the DHT in client mode: they store nothing and answer no DHT queries. Today `/dyapp/kad` is the
store role's key space: `dyapp-node` without the store role runs it in client mode and serves no
protocol. The media role needs the store role and shares its key space for now; other roles get
their own Kademlia protocol name when they are built.

Kademlia gives every node an equal share of keys; a share weighted by the node's capacity is
still to be designed.

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
| `/dyapp/profile` | store | `publish(SignedRecord)`, `get(identity)` |
| `/dyapp/mailbox` | store | `challenge`, `put(envelope)`, `fetch(mailbox)`, `ack(ids)`; node to node `replica_ack`, `inventory(ids)` → `missing(ids)`, `replica_put(envelopes)` |
| `/dyapp/mailbox-push` | client | the node pushes new envelopes to a connected device over its connection |
| `/dyapp/signal` | store | `put(kind, envelope)`, `fetch`, `ack`; one kind per signal store (like, view, …) |
| `/dyapp/media` | media | `keep(signed hash list)`, `put(owner, blob)`, `get(hash)`, `attach(signed chat attachment)`, `release(secret)`; `get(hash, range)` is planned |
| `/dyapp/search` | search | `query(conditions, limit)` → profiles in random order and the conditions applied |
| `/dyapp/inventory` | search | `have(list)` → `need(list)`, for search catch-up |
| `/dyapp/turn` | TURN | `credentials` → short-lived username, password and URLs |

Gossipsub topics `/dyapp/profiles/<n>`, n = H(identity) mod 16, carry `SignedRecord`s and
heartbeats ([Search](#search)). Media and other large payloads go in chunks below the node's
request size limit.

**Authorisation.** The Noise peer ID is a transport key, never an identity
([ADR 0006](../decisions/0006-transport-keys-separate-from-identity.md)), so anything that acts
on an identity's data carries a signature by a key of that identity:

| Request | Signed by | Checked against |
|---------|-----------|-----------------|
| `profile.publish` | owner identity key, over the record | the key in the record |
| `mailbox.fetch`, `mailbox.ack`, `signal.fetch`, `signal.ack` | the mailbox's device key, over the request and a `challenge` nonce bound to this connection | mailbox address = H(device key) |
| `mailbox.put`, `signal.put` | sender's device key, over the envelope | per-key and per-IP-group quotas only; the recipient checks the sender inside the MLS ciphertext |
| `media.keep` | owner identity key, over the versioned list of the owner's hashes | the key; per-owner media quota |
| `media.put` | nothing: the node takes only a blob whose hash the owner's latest `keep` lists | the list and the quota |
| `media.attach` | sender identity key, over the attachment's hashes, release hash and creation time | the key; per-owner media quota and attachment count |
| `media.release` | nothing: the release secret, which only the sender and the recipient know | SHA-256 of the secret against the attachment's release hash |
| `search.query`, `turn.credentials`, `profile.get`, `media.get` | nothing | rate limit per peer ID and IP group |

An ack is signed by the device, so a node forwards it verbatim and the other replicas verify it
themselves; no node trusts another node. Requests other than fetch and ack are idempotent (the
same id or hash stores once), so replays need no nonce.

### Replication and repair

Profiles, mailbox messages and signals are replicated whole to R = 5 points, replica *i* on the
nodes closest to H(key ‖ i); large media are erasure-coded into K = 6 + M = 4 shards; thumbnails
are stored whole ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).
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
- **Budget.** One inventory per mailbox per hour on a node; repair traffic counts towards the
  monthly cap and stops at 75 % of it, together with media.
- **Scale.** 120 million users on 1200 nodes with R = 5 put about 500 000 users and 750 000
  device mailboxes (1.5 devices per user) on a node. If half the devices come online daily, a
  node sends about 4 inventories a second to 4 peers each, a few hundred bytes apiece (16-byte
  ids of pending messages); envelopes move only for real gaps.

### Profiles

- Payload at most 1 MiB, raisable: nodes state their limit and clients publish only to nodes that
  accept the size. Media are separate blobs; the profile only links to them by content hash.
- Each photo in `photos` is the full image (one or more blobs) plus a separate small thumbnail
  blob, stored whole. Lists and search results load only thumbnails; the full image loads when a
  profile is opened (implemented in the schema and checks; no client yet).
- Location is `place`, a city or district name, never coordinates
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).
- The profile lists the owner's device keys, so a sender can reach every device's mailbox before
  any MLS group exists; anyone can see how many devices a user has.
- The profile carries a proof of work bound to the owner's key, computed once when the key is
  created ([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)).
- Deletion is a signed tombstone kept for the TTL
  ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

### Search

- **Dissemination.** The owner's client publishes its `SignedRecord` on every change to one of a
  fixed number of gossipsub topics chosen by its key (for example H(key) mod 16). It publishes
  without subscribing, so a phone does not receive the network's updates. Once a day it also
  publishes a signed heartbeat (key, profile version, date, about 100 bytes). Nodes check the
  signature before relaying, relay only newer versions and limit updates per key.
- **Index.** A search node subscribes to all topics by default, or to a part and then holds a
  uniform random sample. It indexes only profiles with enough proof of work (the minimum
  difficulty is a node setting) and keeps as many as its limits allow, evicting by the heartbeat
  date. A node that was offline catches up by exchanging (key, version) lists with other search
  nodes.
- **Queries.** A client asks one to three search nodes. Results come in random order, up to a
  per-reply limit set by the node; a repeated query gives a new sample, there is no pagination.
  A client that does not find what it wants asks another node. Queries are rate-limited per
  requesting key (node setting); scraping public profiles is accepted.
- **Schema without migrations.** Hot fields live in a typed table; other attributes are indexed
  as (attr_id, int64 value) pairs, at most N per profile, one value per attr_id, attr_id in a
  bounded range, or the profile is rejected. The index has a version; on a bump the node builds
  the new index from the stored payloads next to the old one, keeps answering from the old one
  and switches when the new one is ready. A node skips conditions it does not understand,
  returns a superset and lists the conditions it applied; the client filters the rest. `place`
  matches exactly within the country.

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

- One SQLite file per data type ([ADR 0015](../decisions/0015-sqlite-node-storage.md)): `profiles.db`, `profile-index.db` (disposable, rebuilt),
  `messages.db`, `likes.db`, `views.db` and one more per new signal type; `admin.db` for operator
  settings. No transaction spans two stores.
- Media blobs are files, never database rows: `<media dir>/aa/bb/<hash>`, written to a temporary
  file, fsync'd and renamed; the name is the SHA-256 of the data. Implemented, without an fsync
  of the directory ([Storage](#storage)).
- No routine full `VACUUM`: stores use `auto_vacuum = INCREMENTAL` with `incremental_vacuum(N)`,
  a bounded WAL (`journal_size_limit`, regular checkpoints) and `PRAGMA optimize`, on a per-store
  schedule with an optional maintenance window and an I/O budget. Implemented: one schedule for
  all stores with a page budget per run ([Storage](#storage)); per-store schedules, the window
  and a byte/time I/O budget are planned.
- Activity: one UPSERT of a user's last-active date per day.

### Limits and abuse

Defaults are node settings: a message up to 100 KB, a mailbox up to 10 MB per device, media up
to 10 MB per user, each counted by the node over the data it holds. Write quotas apply per sender
key and per IP group; the prefix length (for example /24 or /48) is the operator's choice.
Resource guards cap disk per store and for media (with a free-space reserve), traffic (rates and
an optional monthly cap; near it the node sheds media first, then search, the mailbox last) and
memory (connections, streams, request size). A full store answers "full" so the client tries
another replica. Implemented: the disk, monthly-traffic and connection guards in
[Resource guards](#resource-guards) and the quotas and peer bans in [Rate Limiting](#rate-limiting);
byte rates and a memory threshold are planned.

The operator may refuse service to any user through a deny list (implemented, see
[Deny list](#deny-list)). Lists may be shared between operators but are advisory: a node never
has to follow another's list. Removing illegal media on request is still to be designed.

### Operating a node

- Runs as an unprivileged system user and refuses to start as root; default port 7070, every
  path must be writable by that user (implemented in `dyapp-node`). The Debian 12 package adds a
  hardened systemd unit and the `dyapp-node` system user
  ([deployment](../operations/deployment.md#debian-12-package)).
- A TOML config sets addresses, paths per store, roles, limits and TTL; environment variables
  override it and unknown keys are ignored, so a rolled-back node still starts. Invalid config
  fails at startup ([Configuration](#configuration)).
- The node key is a libp2p key file (mode 0600) in the data directory, created on first start.
  A new key is a new node: the node refuses a key that does not match the stored data, and the
  operator deletes the data. On a leak or a move the operator creates a new key.
- Administration is a local CLI that writes to `admin.db`; the node applies changes without a
  restart and logs them. Metrics go to the log; there is no HTTP endpoint.

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
(`dyapp-node` deletes expired envelopes every hour) → fetched oldest first.

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
Profiles have no TTL.
Full field table: [Privacy & Metadata Visibility](../security/privacy.md).

## Served protocol

Source: `rust/bootstrap/src/service.rs` (request handling), `node.rs` (the libp2p
loop), `rust/p2p-net` (`ProtoCodec`, protocol IDs), `storage.rs` (SQLite), `media.rs` (blobs).
`dyapp-node` serves these libp2p request-response protocols over TCP and QUIC, one protobuf request and one reply
per stream, at most 2 MiB each ([schema](protobuf-schema.md#node-protocol)):

| Protocol | Request | Reply |
|----------|---------|-------|
| `/dyapp/node` | `info` | `STATUS_OK`, roles (`ROLE_STORE`, `ROLE_MEDIA`), `max_media_bytes` (1 MiB, media role only), `max_profile_bytes` (1 MiB), `max_message_bytes` (100 KiB), `max_mailbox_bytes` (10 MiB), `retention_seconds` (`limits.message_ttl_hours`); a node without the store role serves no protocol |
| `/dyapp/profile` | `publish(SignedRecord)` | `OK`; `STALE` with the stored record when the version is not newer; `DENIED` bad signature; `INVALID` bad key or content; `TOO_LARGE` payload over 1 MiB |
| `/dyapp/profile` | `get(peer_id)`, 32 raw bytes | `OK` with the record, tombstone included; `NOT_FOUND`; `INVALID` wrong length |
| `/dyapp/mailbox` | `challenge` | `OK` with a fresh 32-byte nonce for this connection; it replaces the previous one |
| `/dyapp/mailbox` | `put(SignedRecord)`, payload `Envelope` | `OK`, also for a repeated (mailbox, id); `DENIED` bad signature; `INVALID` undecodable, id not 16 or mailbox not 32 bytes; `TOO_LARGE` payload over 100 KiB; `FULL` the mailbox would exceed 10 MiB |
| `/dyapp/mailbox` | `fetch(SignedRecord)`, payload `Fetch` | `OK` with the oldest envelopes (at most `limit`, 100 and 1 MiB per reply) and `more`; `DENIED` |
| `/dyapp/mailbox` | `ack(SignedRecord)`, payload `Ack` | `OK`, the listed ids are deleted, unknown ones ignored, and the ack is forwarded as `replica_ack`; `DENIED`; `INVALID` an id not 16 bytes |
| `/dyapp/mailbox` | `replica_ack(SignedRecord)` | an `ack` another node forwards verbatim: checked like `ack` without the nonce, not forwarded again; same replies |
| `/dyapp/mailbox` | `inventory(Inventory)`: mailbox, at most 10 000 ids (node to node) | `OK` with `missing`, the listed ids the node does not hold; `INVALID` mailbox not 32 bytes, an id not 16 or too many ids; `REFUSED` a denied mailbox; `RATE_LIMITED` from 75 % of the traffic cap |
| `/dyapp/mailbox` | `replica_put(Envelopes)` (node to node) | each envelope stored like a `put`; `OK`, else `DENIED` when any signature is forged, else the first failure |
| `/dyapp/mailbox-push` | `MailboxPush` (node to client) | the envelope just stored, sent once per connection that sent a `fetch` with `watch` |
| `/dyapp/media` | `keep(SignedRecord)`, payload `MediaKeep` | `OK` with `missing`, the listed hashes the node does not hold yet; `STALE` version not newer; `DENIED` bad signature; `INVALID` over 256 hashes or a hash not 32 bytes |
| `/dyapp/media` | `put(MediaPut)`: owner = SHA-256 of the identity key, data | `OK`, also for a blob already held; `NOT_FOUND` the owner's list lacks SHA-256(data); `TOO_LARGE` over 1 MiB; `FULL` over `limits.media_per_owner_mb` or the media disk guard; `INVALID` owner not 32 bytes |
| `/dyapp/media` | `get(GetMedia)`: hash | `OK` with the blob; `NOT_FOUND`; `INVALID` hash not 32 bytes |
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
  `replica_key(mailbox, i)` and sends it the signed ack as `replica_ack`; replies are ignored.
  Only the closest node per key gets it, and a node that missed it keeps the envelope until its
  TTL. A replayed `replica_ack` can only delete ids the device already acked.
- **Mailbox repair.** A valid `fetch` with `watch` starts a repair of that mailbox, at most once
  an hour per node and only below 75 % of the traffic cap. The node looks up the closest node of
  each `replica_key(mailbox, i)`, skipping keys it is closer to itself, and sends it an
  `inventory` of the ids it holds; the envelopes the answer lists as `missing` follow in one
  `replica_put`. The peer, if one of the first five nodes its own routing table holds for a
  replica key is the requester, sends back the envelopes the inventory lacks the same way. A
  batch is cut at 1 MiB, the rest waits for the next visit; repair bytes count towards the
  traffic cap. A repaired envelope gets a fresh TTL on its new node. Old nodes answer both
  requests `UNSUPPORTED`, and repair skips them.
- **Media.** The owner's signed `keep` is the whole list of blobs the node should hold for that
  identity, replaced by a higher `version`; a blob dropped from every owner's list is deleted at
  once. A put needs no signature: the list authorises it, and a replay stores nothing new. A blob
  kept by two owners counts against both quotas. There is no eviction: a full node answers
  `FULL` and the client tries another. Media requests have their own per-peer limit
  (`limits.media_requests_per_second`, default 10, no strike) besides the shared ones. A node
  without the media role answers `UNSUPPORTED`. Ranges, media replication and repair are
  planned.
- **Chat attachments.** A signed `attach` lists up to 16 blobs under the SHA-256 of a release
  secret; the blobs are put like listed ones and count against the sender's quota. Anyone holding
  the secret (the recipient, after downloading) sends `release` and the node drops the
  attachment at once; a blob also kept or attached elsewhere stays. An unreleased attachment
  expires `limits.attachment_retention_hours` (default 168) after its `created` time, which may
  be at most 10 minutes ahead of the node's clock; the hourly cleanup deletes it. A sender has at
  most 256 unreleased attachments per node (`FULL` beyond). A replayed `attach` restores the
  attachment until that same expiry. Nodes older than attachments answer both requests
  `UNSUPPORTED`.
- <a id="deny-list"></a>**Deny list.** `<storage.dir>/deny` holds one entry per line: a libp2p
  peer ID, an IP group as the node computes it (for example `203.0.113.0/24` with the default
  prefix) or the lowercase hex SHA-256 of an identity or device key; `#` starts a comment. The
  node reads it at start and on SIGHUP and logs the entry count and how many were added and
  removed; an unreadable file keeps the old list. A listed peer or IP group gets `REFUSED` on
  every profile, mailbox and media request; a listed key gets `REFUSED` on its profile publish
  and get, on puts it signs or addressed to its mailbox, on its fetch and on its media `keep` and
  `put`. `REFUSED` is no strike, so the client moves to another replica; one node's list removes
  no one from the network. Acks are still served, and a `get` of a listed owner's blob already
  stored is still answered (blobs are not indexed by owner on read).
- **Not yet:** the size limits are constants rather than config ([Mailboxes](#mailboxes)).
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
```

`put` signs with a fresh sender key; `fetch` and `ack` get a challenge and use it on one
connection. `closest` lists the nodes that answered, closest first. `replicate` publishes each of
the `REPLICAS` replicas of a profile to the node closest to `replica_key(peer_id, i)`; a second
replica on the same node answers `STALE`, which counts as stored. `put-replicas` puts one
envelope to the node closest to each `replica_key(mailbox, i)`. `watch` sends a watching `fetch`,
puts an envelope to its own mailbox on another connection and prints the ids pushed back. `flood`
sends `n` profile gets at once on one connection.

It has no DNS transport: pass `/ip4/` or `/ip6/` addresses, without `/p2p/`.

## Replication Strategy (Reed-Solomon)

**Status:** `rust/bootstrap/src/replication.rs` only encodes a byte buffer into
shards and decodes it back, in one process. Nothing in `service.rs` or `storage.rs`
calls it, the node config has no replication setting, and there is no
transport between servers. Three separate pieces are needed for real
replication, and only the first exists:

1. **Erasure coding** (exists): split a record into `data` + `parity` shards.
2. **Node↔shard assignment** (planned): decide which server stores which shard.
3. **Replication protocol** (planned): send shards to peers, track their
   state, rebuild after a failure.

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

The target code is fixed by the protocol: K=6 data + M=4 parity shards on 10
nodes, surviving the loss of any 4 (40%), used for large media only;
messages and profiles are replicated whole [ADR 0009](../decisions/0009-message-delivery-and-storage.md). This mapping
does not produce it.

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

Codec unit tests show that a buffer can be rebuilt with one shard missing.
They do not show that a cluster recovers data, because no cluster exists.

## Rate Limiting

`rate_limit.rs` keeps one `governor` token bucket per key with
`Quota::per_second(n)`; `n` is passed by the host program to
`PeerRateLimiter::new(n)` (`dyapp-node` passes `limits.requests_per_second`, default 100).
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
  `min_free_mb` free on the media file system; media is never evicted.
- **Traffic**: node protocol bytes in and out (encoded requests and replies, not transport
  overhead) are counted per UTC calendar month and saved to `<storage.dir>/traffic` at each
  maintenance run. With `monthly_traffic_gb` set (default 0, no cap), media requests get
  `RATE_LIMITED` from 75 % of the cap, profile requests from 90 % and mailbox requests at 100 %;
  `info` is always served. Per-second byte rates are planned; search, once served, is shed
  before profiles.
- **Connections and memory**: libp2p connection limits — `max_connections` established and
  pending incoming (default 1000), `max_connections_per_peer` (default 4) — and `max_streams`
  concurrent streams per connection and protocol (default 16); messages are capped at 2 MiB.
  Refused connections are counted and logged at each maintenance run. A memory-use threshold
  is planned.

## Storage

One SQLite file per data type in the storage directory, WAL journal,
`auto_vacuum = INCREMENTAL`, `journal_size_limit` 64 MiB:

```
profiles.db  profiles(peer_id PK, record BLOB, live)
             record: SignedRecord (protobuf), latest version or tombstone (live = 0)
messages.db  envelopes(seq PK, mailbox BLOB, id BLOB, record BLOB, size, expires_at,
                       UNIQUE (mailbox, id))
             record: the signed envelope as received; indexes on (mailbox, seq), expires_at
<media>/media.db  owners(owner PK, version)  blobs(owner, hash, size, PK (owner, hash))
             owner: hex SHA-256 of the identity key; size NULL = listed, not yet put
<media>/aa/bb/<hex hash>  the blob
```

A `messages` table left by an older version is not read; it can be dropped by hand.
`put_envelope` sums the mailbox's `size` on each put under the store lock.

Each file has one connection behind a mutex. There is no schema version and no
format change path yet; the target builds a new format next to the old one
([Principles](#principles)).

**Compaction:** `BootstrapStore::cleanup_expired` deletes expired envelopes; `dyapp-node` calls it every hour. Profiles and tombstones have no expiry; under a full quota `evict_profiles` and `evict_envelopes` delete the oldest rows by rowid ([resource guards](#resource-guards)). `INSERT OR REPLACE` gives a republished profile a new, highest rowid, so rowid order is publish order. A profile retention TTL is planned ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).

**Maintenance:** every `maintenance.interval_minutes` (default 60) `dyapp-node` calls
`BootstrapStore::maintain`, which, one store at a time under its lock, frees at most
`maintenance.vacuum_pages` free pages (default 2048 = 8 MiB) with `incremental_vacuum`,
checkpoints the WAL with `TRUNCATE` and runs `PRAGMA optimize`, then logs the free pages left.
There is no full `VACUUM`: a large freelist shrinks over several runs, and requests to that store
wait for one run only. Not measured yet: fragmentation under load, and whether time-bucketed
message files beat deletes.

`put_profile` holds a lock across read-compare-write, so two concurrent uploads for one peer cannot both win.

## Configuration

`NodeConfig` (`rust/bootstrap/src/config.rs`) is read by `dyapp-node`: defaults, then a TOML
file (`--config`), then `DYAPP_NODE__<SECTION>__<KEY>` variables. Every field has a default and
unknown keys are logged and ignored, so configs work across upgrades and rollbacks. Keys:
`listen`, `external`, `seeds` ([joining](p2p-networking.md)), `roles`, `storage.{dir,profiles,messages,media}`
(media defaults to `<dir>/media`),
`limits.{message_ttl_hours,requests_per_second,media_requests_per_second,media_per_owner_mb,attachment_retention_hours}`,
`limits.{profiles_max_mb,messages_max_mb,media_max_mb,min_free_mb,monthly_traffic_gb}`,
`limits.{max_connections,max_connections_per_peer,max_streams}`
([Resource guards](#resource-guards)),
`limits.{ip_group_requests_per_second,ipv4_prefix,ipv6_prefix,sender_puts_per_second,strikes_to_ban,ban_minutes}`
([Rate Limiting](#rate-limiting)), `maintenance.{interval_minutes,vacuum_pages}`; the example and startup checks are in
[Deployment](../operations/deployment.md#dyapp-node). `dyapp-node` starts the libp2p node
([P2P networking](p2p-networking.md)) in `Mode::Auto` with the stores open and serves
the protocols in [Served protocol](#served-protocol). Only the `store` and `media` roles are
accepted, and `media` only together with `store`.

Planned: a maintenance window and per-store schedules, the remaining resource guards, TURN ports,
store retention.

## Deployment Model

### Single Node (Development)

```
Client A ──┐
Client B ─→ Bootstrap (single instance)
Client C ──┘
```

**Limitation:** if node fails, all queued messages lost.

### Multi-Node Cluster (target, not implemented)

There is no cluster transport, node-to-shard assignment, failover or
multi-master protocol. Several servers behind the same DNS name with the same
`replication_factor` are independent: nothing synchronises their data, and a
client may write to one and read from another. A cluster setup requires an
implemented replication protocol and a node-failure test first. The target is
described in [Target Design](#target-design--planned).

## Security

### What bootstrap CAN do

✅ See peer IDs, client IP addresses, who messages whom and when
✅ See every profile field (public by design)
✅ See message content too, until client crypto exists
✅ Rate-limit requests per libp2p peer ID and IP group and ban misbehaving peers locally (no Sybil resistance, see [Rate Limiting](#rate-limiting))

DHT node role is not implemented.

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

There is no health endpoint: `/dyapp/node` `info` answering is the liveness check. Planned:
metrics as log lines and a local admin CLI, no HTTP endpoints; see the
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
- Bad input fails only its request: undecodable protobuf and a request over 2 MiB get the
  stream closed without a reply (a client reads `STATUS_UNSPECIFIED`), an unknown or empty
  variant gets `UNSUPPORTED`, and the node then answers the next client
- Rate limit over the wire: `RATE_LIMITED` past the burst, `info` still answered

Integration tests (`tests/integration_tests.rs`, in-process, no network):
- Replication codec with one lost shard (`test_replication_fault_tolerance`)


The network test (`scripts/network-test.py`) runs two `dyapp-node` instances and drives them
with `test-peer`: `info` over TCP and QUIC, publish over TCP and get over QUIC with identical
bytes, `STALE` on replay; a mailbox put over TCP and fetch over QUIC, a stranger's key reads
nothing, ack empties it; and that the nodes do not share storage. No test covers a
multi-node cluster or
a node failure.

## Limitations & Future

**Current limitations:**
- Single-node persistence: no HA, failover or cross-node replication (the RS codec is not wired into storage or the API)
- Mailbox replicas are written by the client; nodes forward acks and repair gaps only when a device watches its mailbox
- No audit logging

Planned changes: see [Target Design](#target-design--planned).
