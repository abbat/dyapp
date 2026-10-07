# Bootstrap Server Architecture (Distributed Relay)

## Overview

A bootstrap server stores offline messages and owner-signed profiles over REST.
Peer discovery, profile search and signaling are planned, not implemented.

> ⚠️ The server stores whatever bytes clients send in `encrypted_payload`.
> Clients do not encrypt yet, so the operator sees plaintext. Message routes have
> no authentication; profile writes are accepted only with the owner's signature.
> See [Encryption & Security Status](../security/encryption.md).

**Key principles:**
- **Replication (target)**: messages and profiles replicated whole to 5 points, Reed-Solomon K=6/M=4 only for large media [ADR 0009](../decisions/0009-message-delivery-and-storage.md); today only a local encode/decode codec exists and each server is a single node (see [Replication](#replication-strategy-reed-solomon))
- **Message relay**: storage with a TTL field (default 24h; not enforced yet, see [Privacy](../security/privacy.md#retention))
- **Profile storage**: public profiles signed by the owner's identity key; the highest version wins and deletion is a signed tombstone ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); no search endpoint yet
- **Rate limiting**: per-ID token bucket on the two POST endpoints only (see [Rate Limiting](#rate-limiting))
- **Encryption (target)**: clients end-to-end encrypt messages and media before upload; profiles are public and signed, not encrypted ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); not implemented
- **Network (target)**: open, anyone may run a node, DHT discovery ([ADR 0007](../decisions/0007-open-bootstrap-network.md)); storage is a cache with an operator-set retention TTL (default 30 days) and eviction by profile activity ([ADR 0009](../decisions/0009-message-delivery-and-storage.md))

## Target Design — planned

Nothing in this section is implemented; the sections after it describe today's code.

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
the DHT in client mode: they store nothing and answer no DHT queries.

Kademlia gives every node an equal share of keys; a share weighted by the node's capacity is
still to be designed.

### Protocol

One libp2p request-response protocol per service, protobuf requests and replies, schemas in
`proto/`. Each request is a `oneof`; a node that gets a variant it does not know answers
`unsupported` and the client tries another node. Every reply carries a status: `ok`, `not_found`,
`stale`, `too_large`, `full`, `rate_limited`, `denied`, `unsupported`, `invalid`.

| Protocol | Role | Requests |
|----------|------|----------|
| `/dyapp/node` | all | `info`: roles, limits (payload, message, mailbox, media), supported search filters, minimum profile proof of work, retention TTL |
| `/dyapp/profile` | store | `publish(SignedRecord)`, `get(identity)` |
| `/dyapp/mailbox` | store | `challenge`, `put(envelope)`, `fetch(mailbox)`, `ack(ids)` |
| `/dyapp/mailbox-push` | client | the node pushes new envelopes to a connected device over its connection |
| `/dyapp/signal` | store | `put(kind, envelope)`, `fetch`, `ack`; one kind per signal store (like, view, …) |
| `/dyapp/media` | media | `put(hash, chunk)`, `get(hash, range)`, `downloaded(hash)` |
| `/dyapp/search` | search | `query(conditions, limit)` → profiles in random order and the conditions applied |
| `/dyapp/inventory` | store, media, search | `have(list)` → `need(list)`, for repair and search catch-up |
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
| `media.put`, `media.downloaded` | owner (or recipient) device key, over the hash | per-user media quota |
| `search.query`, `turn.credentials`, `profile.get`, `media.get` | nothing | rate limit per peer ID and IP group |

An ack is signed by the device, so a node forwards it verbatim and the other replicas verify it
themselves; no node trusts another node. Requests other than fetch and ack are idempotent (the
same id or hash stores once), so replays need no nonce.

### Replication and repair

Profiles, mailbox messages and signals are replicated whole to R = 5 points, replica *i* on the
nodes closest to H(key ‖ i); large media are erasure-coded into K = 6 + M = 4 shards; thumbnails
are stored whole ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).

Repair is driven by the owner's presence. When a user comes online, the nodes responsible for
their keys exchange inventories, *I have* (profile version, message ids, media hashes) and
*I need*, and fill the gaps, so new closest nodes get the data after churn. Nodes never rebuild
media shards: the owner re-uploads missing ones. Data of a user who stays offline is not
repaired and expires with the TTL.

### Profiles

- Payload at most 1 MiB, raisable: nodes state their limit and clients publish only to nodes that
  accept the size. Media are separate blobs; the profile only links to them by content hash.
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
- Chat attachments are encrypted media blobs; the recipient's "downloaded" signal lets nodes
  delete them, and unconfirmed ones expire after an operator-set retention.

### Storage on a node

- One SQLite file per data type ([ADR 0015](../decisions/0015-sqlite-node-storage.md)): `profiles.db`, `profile-index.db` (disposable, rebuilt),
  `messages.db`, `likes.db`, `views.db` and one more per new signal type; `admin.db` for operator
  settings. No transaction spans two stores.
- Media blobs are files, never database rows: `<media dir>/aa/bb/<hash>`, written to a temporary
  file, hash-checked, fsync'd and renamed.
- No routine full `VACUUM`: stores use `auto_vacuum = INCREMENTAL` with `incremental_vacuum(N)`,
  a bounded WAL (`journal_size_limit`, regular checkpoints) and `PRAGMA optimize`, on a per-store
  schedule with an optional maintenance window and an I/O budget.
- Activity: one UPSERT of a user's last-active date per day.

### Limits and abuse

Defaults are node settings: a message up to 100 KB, a mailbox up to 10 MB per device, media up
to 10 MB per user, each counted by the node over the data it holds. Write quotas apply per sender
key and per IP group; the prefix length (for example /24 or /48) is the operator's choice.
Resource guards cap disk per store and for media (with a free-space reserve), traffic (rates and
an optional monthly cap; near it the node sheds media first, then search, the mailbox last) and
memory (connections, streams, request size). A full store answers "full" so the client tries
another replica.

The operator may refuse service to any user through a deny list. Lists may be shared between
operators but are advisory: a node never has to follow another's list. Removing illegal media
on request is still to be designed.

### Operating a node

- Runs as an unprivileged system user and refuses to start as root; default port 7070, every
  path must be writable by that user (implemented in `dyapp-node`). The Debian package adds a
  systemd unit with hardening (planned).
- A TOML config sets addresses, paths per store, roles, limits and TTL; environment variables
  override it and unknown keys are ignored, so a rolled-back node still starts. Invalid config
  fails at startup ([Configuration](#configuration)). Maintenance settings are planned.
- The node key is a libp2p key file (mode 0600) in the data directory, created on first start.
  A new key is a new node: the node refuses a key that does not match the stored data, and the
  operator deletes the data. On a leak or a move the operator creates a new key.
- Administration is a local CLI that writes to `admin.db`; the node applies changes without a
  restart and logs them. Metrics go to the log; there is no HTTP endpoint.

## Data Model

### MessageBlob (Offline Queue)

```rust
pub struct MessageBlob {
    id: String,                    // UUID
    sender_id: String,             // Sender's peer ID
    recipient_id: String,          // Recipient's peer ID
    encrypted_payload: Vec<u8>,    // opaque bytes; E2E encryption planned
    timestamp: i64,                // Creation time
    ttl_expires_at: i64,           // Auto-delete time
}
```

**Lifecycle:**
```
Sender offline
  ↓
Store on bootstrap
  ↓
(planned) Replicate via erasure coding
  ↓
Recipient queries bootstrap
  ↓
Deliver message
  ↓
TTL expires (24h default)
  ↓
Auto-delete (planned: cleanup_expired is not scheduled yet)
```

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

`POST /profiles` stores a record only if:

- the signature verifies against `public_key`;
- the payload is at most 1 MiB and decodes as a `Profile` (media are separate blobs, planned);
- `version` ≥ 1 and greater than the stored version for that peer ID (otherwise `409`);
- content is sane: country is an ISO 3166-1 alpha-2 code, income range not reversed,
  place at most 1024 characters without control characters, a tombstone (`deleted = true`) carries no other field.

Deletion publishes a tombstone with a higher version. The server keeps the tombstone
so an older version cannot be re-imported; `GET /profiles` skips it, `GET
/profiles/{peer_id}` returns it so peers learn of the deletion. Profiles have no TTL.
Full field table: [Privacy & Metadata Visibility](../security/privacy.md).

## REST API

Source: `rust/bootstrap/src/api.rs` (routes), `storage.rs` (SQLite),
`error.rs` (error mapping). Behaviour below was checked against a running
`test-peer` in the `dyapp:network-test` container.

**Common to all endpoints:**

- **No authentication or authorization** on message routes; any client can call
  them. Profile writes need the owner's signature, reads are open.
  CORS is permissive (`Access-Control-Allow-Origin: *`); transport is plain HTTP.
- **Message bytes are JSON arrays of numbers.** `encrypted_payload` is `Vec<u8>`
  and serializes as `[104,101,...]`, not base64. The server stores it as sent;
  the field name does not mean the content is encrypted.
- **Profile bodies are protobuf** (`Content-Type: application/x-protobuf`): a
  `SignedRecord` for one profile, a `ProfileList { repeated SignedRecord profiles = 1 }`
  for the list. The request `Content-Type` is not checked.
- **JSON body errors on message routes come from axum, not `error.rs`:** malformed
  JSON → `400`, missing or wrong-typed field → `422`, missing
  `Content-Type: application/json` → `415`; the body is plain text, not `{"error": ...}`.
- **Server errors** (`StorageError`, `SerializationError`, …) → `500`
  `{"error": "<message>"}`.
- **Message TTL is stored, not enforced.** `ttl_expires_at` is set on write, but
  no read path checks it and `cleanup_expired` has no callers, so expired messages
  are still returned until explicitly deleted.

| Endpoint | Request | Success | Errors | Rate limit | TTL |
|----------|---------|---------|--------|------------|-----|
| `POST /messages` | `{"sender_id", "recipient_id", "encrypted_payload": [u8]}` | `201 {"message_id": "<uuid>"}` (ID generated by the server) | `429 {"error":"Rate limit exceeded"}`; 400/415/422 | yes, keyed by `sender_id` | sets `now + message_ttl_hours` |
| `GET /messages/{message_id}` | — | `200 MessageBlob` | `404 {"error":"Message not found"}` | no | not checked |
| `DELETE /messages/{message_id}` | — | `204`, also when the ID does not exist | — | no | — |
| `GET /messages/peer/{peer_id}` | — | `200 [MessageBlob]` where `recipient_id == peer_id` (full scan of `msg:` keys; no pagination; messages stay stored after read) | — | no | not checked |
| `POST /profiles` | protobuf `SignedRecord` | `201 {"peer_id", "version"}`; replaces the stored version | `400 {"error"}` bad protobuf, key, signature or content; `409` version not newer; `429` | yes, keyed by the peer ID of `public_key` | none |
| `GET /profiles/{peer_id}` | — | `200` protobuf `SignedRecord`, tombstone included | `404 {"error":"Profile not found"}` | no | — |
| `GET /profiles?skip=&limit=` | `skip` default 0, `limit` default 100, no upper bound | `200` protobuf `ProfileList` in peer-ID order, tombstones skipped | 400 on non-numeric query | no | — |
| `GET /health` | — | `200 {"status":"healthy","timestamp"}` | `503 {"status":"unhealthy","timestamp"}` | no | — |

There is no search/filter endpoint (age range, place), no peer discovery
endpoint (`/api/peers`), no DHT, and no SDP/ICE signaling endpoint.

### Trusted vs untrusted fields

Message fields are validated only for JSON types. A client must treat every
message field it reads back as **claimed by some other client**. Profile content is
signed, so a client re-verifies a fetched record (`dyapp_profile::verify`) and
then trusts it as the owner's own claim, not as fact:

| Field | Who sets it | What the server checks |
|-------|-------------|------------------------|
| `sender_id`, `recipient_id` | client | nothing; any string, no proof of key ownership |
| `encrypted_payload` | client | nothing; plaintext is accepted |
| profile peer ID | server | derived from the signing key |
| profile fields | owner | signature, version order, format checks above |
| `age` | owner | nothing beyond `u32`; no minimum, so an age below 30 or 18 is stored |
| `id` (message), `timestamp`, `ttl_expires_at` | server | generated on write |

Consequences:

- **Self-asserted message peer IDs.** Anyone can read any peer's inbox
  (`GET /messages/peer/{peer_id}`) or delete any message.
- **Profiles are owner-only.** Only the holder of the identity key can publish,
  replace or delete (tombstone) a profile. A replayed old version is rejected.
- **Age.** Age is self-declared; the client refuses users under 18 in its UI,
  and 30+ is only the target audience
  ([ADR 0012](../decisions/0012-private-p2p-interactions.md)). The server checks
  nothing beyond the signature: it would accept `17`.

Signed message requests are planned; see
[Encryption & Security Status](../security/encryption.md).

### Example

```
POST /profiles
Content-Type: application/x-protobuf

<SignedRecord bytes>
→ 201 {"peer_id": "<64 hex chars>", "version": 1}
```

`test-peer sign-profile` prints a freshly signed sample profile as hex; the
network test (`scripts/network-test.py`) posts it this way. `test-peer` serves on `TEST_PEER_ADDR`
(default `0.0.0.0:7070`) with storage in `TEST_PEER_STORAGE` (default `/tmp/ai/bootstrap`).

## Replication Strategy (Reed-Solomon)

**Status:** `rust/bootstrap/src/replication.rs` only encodes a byte buffer into
shards and decodes it back, in one process. Nothing in `api.rs` or `storage.rs`
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
`PeerRateLimiter::new(n)` (`test-peer` passes `limits.requests_per_second`, default 100).
The bucket allows a burst of `n` requests and then refills at `n`/second; an
over-limit request gets `429 {"error":"Rate limit exceeded"}`.

Scope and limits (verified with `test-peer`, `n = 100`):

- Only `POST /messages` (key `sender_id`) and `POST /profiles` (key: the peer ID
  of the record's `public_key`, before the signature is checked) are limited. 150 immediate POSTs with one `sender_id` → 103 × `201`, 47 × `429`.
- The key is self-asserted: 150 POSTs with 150 different `sender_id`s → all `201`.
  Identity keys are free to generate, so profile keys rotate just as easily. The
  limiter does **not** prevent spam from a client that rotates IDs and provides
  **no** Sybil resistance.
- `GET` and `DELETE` are not limited (150 immediate GETs → all `200`).
- There is no per-IP limit, and buckets are never removed
  (`cleanup_inactive` is a stub), so the map grows with every new ID.

## Storage

One SQLite file per data type in the storage directory, WAL journal,
`auto_vacuum = INCREMENTAL` (nothing runs `incremental_vacuum` yet):

```
profiles.db  profiles(peer_id PK, record BLOB, live)
             record: SignedRecord (protobuf), latest version or tombstone (live = 0)
messages.db  messages(id PK, sender_id, recipient_id, encrypted_payload BLOB,
                      timestamp, ttl_expires_at)
             indexes on recipient_id and ttl_expires_at
```

Each file has one connection behind a mutex. There is no schema version and no
format change path yet; the target builds a new format next to the old one
([Principles](#principles)).

**Compaction:** `BootstrapStore::cleanup_expired` deletes expired messages; `dyapp-node` calls it every hour, `test-peer` never does. Profiles and tombstones have no expiry; an LRU by profile activity is planned ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).

`put_profile` holds a lock across read-compare-write, so two concurrent uploads for one peer cannot both win.

## Configuration

`NodeConfig` (`rust/bootstrap/src/config.rs`) is read by `dyapp-node`: defaults, then a TOML
file (`--config`), then `DYAPP_NODE__<SECTION>__<KEY>` variables. Every field has a default and
unknown keys are logged and ignored, so configs work across upgrades and rollbacks. Keys:
`listen`, `external`, `roles`, `storage.{dir,profiles,messages}`,
`limits.{message_ttl_hours,requests_per_second}`; the example and startup checks are in
[Deployment](../operations/deployment.md#dyapp-node). `dyapp-node` starts the libp2p node
([P2P networking](p2p-networking.md)) in `Mode::Auto` with the stores open; it serves no
application protocol yet. Only the `store` role is accepted.

Planned: maintenance windows and budgets, resource guard limits, the media directory, TURN ports,
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
✅ Rate-limit POSTs per client-supplied ID (no Sybil resistance, see [Rate Limiting](#rate-limiting))

DHT node role is not implemented.

### What bootstrap could not do once client crypto exists (target)

Today the operator can read and modify every message, and any client can write
or delete any message (no authentication; `sender_id` is trusted as sent). A
node cannot forge or alter a profile: clients verify the owner's signature. It
can still withhold a profile or serve an older signed version.


❌ Read encrypted messages
❌ Decrypt anything (no session keys)
❌ See call content (no signaling endpoint exists; media goes peer to peer)

It could still link peer IDs to IPs and public profile fields; deanonymization is not
prevented. See [Privacy & Metadata Visibility](../security/privacy.md).

## Monitoring & Metrics

Only `GET /health` exists (SQLite write-lock probe). Planned: metrics as log
lines and a local admin CLI, no HTTP endpoints; see the
[Deployment Guide](../operations/deployment.md#monitoring).

## Testing

Unit test coverage:
- Message store/retrieve/expiry (`storage.rs`); peer-inbox query is tested only in integration tests
- Signed profile: version order, forged payload, tombstone hides and blocks older versions (`storage.rs`)
- Rate limiting (single key, separate keys)
- Replication codec encode/decode with a missing shard (in-process; no nodes involved)
- Health check

Integration tests (`tests/integration_tests.rs`, in-process, no network):
- Replication codec with one lost shard (`test_replication_fault_tolerance`)
- Message relay through the store (`test_bootstrap_message_relay`)
- Profile routes through the axum router: `201`, `409` stale, `400` forged and
  malformed, protobuf GET and list, tombstone (`test_signed_profile_over_http`)

The network test posts a signed profile to two `test-peer` containers and checks
the round trip, the `409` on replay and that the peers do not share storage.
No test covers message routes over HTTP, rate-limit `429`, a multi-node cluster or
a node failure.

## Limitations & Future

**Current limitations:**
- Single-node persistence: no HA, failover or cross-node replication (the RS codec is not wired into storage or the API)
- No authentication or access control on messages: any client can read any inbox and delete any message (see [Trusted vs untrusted fields](#trusted-vs-untrusted-fields))
- Expired messages are served until deleted (TTL not enforced)
- No audit logging

Planned changes: see [Target Design](#target-design--planned).
