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
- **Network (target)**: open, anyone may run a node, DHT discovery ([ADR 0007](../decisions/0007-open-bootstrap-network.md)); storage is an LRU cache by profile activity instead of a TTL ([ADR 0009](../decisions/0009-message-delivery-and-storage.md))

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
country, rounded location, income range, kids, goals, interests, …); every field is
optional and public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).

`POST /profiles` stores a record only if:

- the signature verifies against `public_key`;
- the payload is at most 16 KiB and decodes as a `Profile`;
- `version` ≥ 1 and greater than the stored version for that peer ID (otherwise `409`);
- content is sane: country is an ISO 3166-1 alpha-2 code, income range not reversed,
  location within ±90/±180, a tombstone (`deleted = true`) carries no other field.

Deletion publishes a tombstone with a higher version. The server keeps the tombstone
so an older version cannot be re-imported; `GET /profiles` skips it, `GET
/profiles/{peer_id}` returns it so peers learn of the deletion. Profiles have no TTL.
Full field table: [Privacy & Metadata Visibility](../security/privacy.md).

## REST API

Source: `rust/bootstrap/src/api.rs` (routes), `storage.rs` (RocksDB),
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

There is no search/filter endpoint (age range, location), no peer discovery
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
calls it, `BootstrapConfig::replication_factor` is not read, and there is no
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
`PeerRateLimiter::new(n)` (not part of `BootstrapConfig`; `test-peer` uses 100).
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

RocksDB key-value store:

```
Keys:
  msg:{message_id}      → MessageBlob (JSON)
  profile:{peer_id}     → SignedRecord (protobuf), latest version or tombstone
  health:{uuid}         → temporary (health checks)

Iteration:
  Prefix: msg:          → all messages
  Prefix: profile:      → all profiles and tombstones
```

**Compaction:** `BootstrapStore::cleanup_expired` deletes expired messages, but nothing calls it outside tests, so messages are kept until explicitly deleted. Profiles and tombstones have no expiry; an LRU by profile activity is planned ([ADR 0009](../decisions/0009-message-delivery-and-storage.md)).

`put_profile` holds a lock across read-compare-write, so two concurrent uploads for one peer cannot both win.

## Configuration

```rust
pub struct BootstrapConfig {
    pub listen_addr: String,           // "0.0.0.0"
    pub listen_port: u16,              // 7070
    pub storage_path: String,          // "/var/lib/bootstrap"
    pub max_peers: usize,              // 1000
    pub replication_factor: usize,     // 3 (not read by the server)
    pub message_ttl_hours: u32,        // 24
}
```

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
implemented replication protocol and a node-failure test first.

```
        ┌─────────────┐
        │ Bootstrap 1 │
        └─────────────┘
             /   \
            /     \
Client A ◄─       ─► Profile queries
Client B ◄─       ─► Message relay
Client C ◄─       ─► (signaling: undecided)
            \     /
             \   /
        ┌─────────────┐
        │ Bootstrap 2 │
        └─────────────┘
             /   \
            /     \
        ┌─────────────┐
        │ Bootstrap 3 │
        └─────────────┘
```

**Intended (RS(2,1), one shard per node):** any one node can fail and the two
remaining shards reconstruct each record; two failures lose it.

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

Only `GET /health` exists (RocksDB write/delete probe). Metrics, logging and
admin endpoints are planned; see the
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

**Future enhancements:**
- Multi-master replication (gossip protocol)
- Profile search with filters (age range, location radius)
- Message expiration enforcement (batch cleanup)
- TLS for bootstrap-to-bootstrap communication
- Prometheus metrics export
