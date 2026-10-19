# Protobuf Schema

> **Status:** the schemas in `proto/` are compiled into the Rust crates
> ([code generation](../development/protobuf-codegen.md)). `identity.proto` and `profile.proto`
> are the signed profile used today. `node.proto` defines the libp2p node protocol; `dyappd`
> serves `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push` and
> `/dyapp/media`
> ([bootstrap](bootstrap.md#served-protocol)).

## Files

| File | Package | Messages |
|------|---------|----------|
| `proto/identity.proto` | `dyapp.identity` | `SignedRecord` |
| `proto/profile.proto` | `dyapp.profile` | `Profile` |
| `proto/node.proto` | `dyapp.node` | `Status`, `Role`; requests and replies of `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`, `/dyapp/media`, `/dyapp/search`, `/dyapp/inventory` |

The files are the source; comments in them define each field.

## Signed records

`SignedRecord` carries the signer's Ed25519 public key, a payload and a signature over
`domain || payload`. The domain label (for example `dyapp/profile/v1\0`) names what is signed, so
a signature for one kind cannot be replayed as another. The payload is signed and stored exactly
as received, so a node keeps fields it does not understand.

| Payload | Domain | Signed by |
|---------|--------|-----------|
| `dyapp.profile.Profile` | `dyapp/profile/v1\0` | identity key |
| `dyapp.node.Envelope` | `dyapp/envelope/v1\0` | sender key |
| `dyapp.node.Fetch`, `Ack` | `dyapp/mailbox-fetch/v1\0`, `dyapp/mailbox-ack/v1\0` | device key |
| `dyapp.node.MediaKeep` | `dyapp/media-keep/v1\0` | identity key |
| `dyapp.node.MediaAttach` | `dyapp/media-attach/v1\0` | identity key |
| `dyapp.node.Heartbeat` | `dyapp/heartbeat/v1\0` | identity key |

## Profile

Every field is public ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)):
anything private is end-to-end encrypted and never sent as a profile field. Empty strings, unset
optionals and an age of 0 mean "not published". `version` orders updates (the highest validly
signed version wins); `deleted` marks a tombstone with every other field except `pow_nonce` and
`pow_bits` empty. `pow_nonce` and `pow_bits` carry the Argon2id proof of work over the signer's
key (see [Proof of work](bootstrap.md#search)); missing means 0 bits. Tag 5 held
coordinates and is reserved. `photos` link `/dyapp/media` blobs by SHA-256: each `Photo` is the
full image as ordered blobs plus one small `thumbnail` blob for lists and search results. See
the [bootstrap signed profile](bootstrap.md#signed-profile).

## Node protocol

One request message per protocol, each a `oneof`; every reply carries a `Status`. Identifiers
are raw bytes: the peer ID of an identity is SHA-256 of its public key, a mailbox address is
SHA-256 of the device public key (32 bytes each).

- **`/dyapp/node`**: `info` returns the node's roles and limits; zero means "no such limit" or
  "role not served". A search node also reports `search_fields` (the `Profile` field numbers it
  filters on) and `max_search_profile_bytes`.
- **`/dyapp/profile`**: `publish(SignedRecord)`, `get(peer_id)`, `heartbeat(SignedRecord)` and
  node-to-node `replica_put(SignedRecord)` and `inventory(peer_id)` → stored version or
  `STATUS_NOT_FOUND`, without the record. Publish fans out to the five DHT holders and waits for
  two distinct stores, or one local store with no routing peers. `replica_put` verifies and stores
  without forwarding. Equal/newer holder versions yield `STATUS_STALE` with a verified record.
  A heartbeat advances profile liveness to its signed time only when newer and within 10 minutes of the
  node clock. A stale publish or replayed heartbeat does not extend retention. Successful
  publishes and heartbeats start node-driven version repair, once per rolling hour per key;
  only older or missing holders receive a signed record, within the 75 % traffic budget.
- **`/dyapp/mailbox`**: `challenge` returns a nonce bound to the connection; `fetch` and `ack` are
  signed by the mailbox's device key and carry that nonce. `put` stores an `Envelope` once per
  random 16-byte id, so it needs no nonce; the acceptor confirms two distinct replica holders.
  Client profile/mailbox puts cost five request units; pending writes are capped at 64 and
  expire after 10 seconds with `STATUS_FULL`. A node forwards a signed ack to the other replicas
  verbatim as `replica_ack`; they check the signature and mailbox address, not the nonce. For
  repair a node sends `inventory` (the ids it holds) and gets `missing` back; envelopes move
  between nodes as `replica_put`, signed by their senders and checked like a `put`.
- **`/dyapp/mailbox-push`**: after a fetch with `watch`, the node pushes new envelopes over the
  same connection; the device still acknowledges with an ack.
- **`/dyapp/media`**: `keep` is the owner's signed, versioned list of blob hashes (SHA-256) and
  returns the ones still `missing`. Its signed `time` must be within 10 minutes of the node
  clock; the same version and list with a newer time refreshes media liveness, while replaying
  it does not. Inactive owners' keeps expire after `limits.profile_ttl_days`; shared blobs stay.
  `put` sends a listed blob unsigned once, up to 6 MiB. At or below `media.shard_threshold`
  (default 1 MiB), the acceptor forwards whole copies to five DHT points and confirms two
  distinct holders. Above it, K = ceil(size / 1 MiB), M = 4; `MediaManifest` holds the blob
  hash, length and ordered shard hashes. `replica_put(MediaReplicaPut)` carries either whole
  data or a manifest plus the persisted signed keep or attachment. `shard_put(MediaShardPut)`
  carries that same authorized manifest, an index and data; `shard_get(hash, index)` returns
  a shard. Stored whole copies, signed authorizations and individual shards are at most 1 MiB.
  Receivers verify signatures, index, length and SHA-256, require the hash in the latest keep
  (or a valid unexpired attachment), and never forward replica requests. The first manifest
  wins; conflicts are `INVALID`, while a verified whole copy takes precedence.
  Older keep timestamps are valid on replicas within the owner TTL and cannot refresh liveness.
  Sharded writes confirm two distinct manifest holders and K indices; an isolated node can
  confirm its one local manifest. Remaining replicas fill in the background.
  Client puts and owner quotas count bytes × 5 for whole copies or ceil(bytes × (K+M)/K)
  for shards; node puts count their payload against peer limits. `get(hash)` returns a verified
  whole copy or assembles K valid shards and checks the final blob hash, billing reply bytes.
  Assembly uses the shared 75 % traffic budget with at most four active reads and no cache.
  A decoded hash mismatch deletes only local manifest/shards and returns `NOT_FOUND`;
  ownership lists stay for reupload. All selected store holders must also serve media while
  these roles share one DHT.
  An accepted `keep` starts bounded node repair before returning `missing`: up to 256 hashes,
  one `have(MediaHave)` per current holder, one inventory per hash per rolling hour.
  `MediaPart` has a blob hash and optional shard index; no index means whole copy/manifest.
  `MediaResponse.inventory` partitions allowed parts into `present` and `missing`, includes
  manifests and names available whole copies. Untrusted peers or peers outside the nearest
  replica-key neighbourhood receive no entries. Only missing items are written; rebuilding
  verifies the final blob hash and carries the signed keep. Repair shares the 75 % assembly
  budget and media dispatch limits. With no copy/K indices, or an incomplete check, the keep
  reply requests a client reupload. There are four active keeps with fifteen-second deadlines;
  local availability without repair remains the fallback when admission is unavailable.
  `attach` is a signed chat attachment: blob hashes, the SHA-256 of a release secret and a
  creation time; `release(secret)` drops it on that node, so release every replica holder.
- **`/dyapp/search`** (experimental search role): `publish(SignedRecord)` and `heartbeat(SignedRecord)`
  as on `/dyapp/profile`; `Heartbeat.version` names the owner's current profile version, and a
  search node holding an older one answers `STATUS_NOT_FOUND` (store nodes ignore the field).
  `query(SearchQuery)` takes up to 16 `Condition`s (`OP_EQ`, `OP_IN` with up to 8 values,
  inclusive `OP_RANGE`) and returns `SearchHit`s (key hash, version), the `applied` fields and
  `partial` when the scan budget ran out; `get(SearchGet)` returns up to 50 records, cut at 1 MiB.
- **`/dyapp/inventory`** (experimental search role): node to node, `InventoryRequest(topic, after)`
  returns up to 10 000 `SearchHit`s of that gossip topic sorted by key, for catch-up.

### Rejection hints

The response schemas reserve additive hints for limits checked by the receiving node:

| Message | Field | Tag |
|---------|-------|-----|
| `ProfileResponse` | `max_bytes` | 4 |
| `MailboxResponse` | `max_bytes` | 6 |
| `MediaResponse` | `max_bytes` | 5 |
| `SearchResponse` | `max_bytes` | 8 |
| `SearchResponse` | `required_pow_bits` | 9 |

`max_bytes` describes the size bound for a `STATUS_TOO_LARGE` rejection;
`required_pow_bits` describes the node's profile PoW threshold for a `STATUS_INVALID`
rejection caused by insufficient PoW. Zero means not reported, including replies from older
nodes. Mailbox replies now report the exact checked signed-envelope payload bound for client
puts, local stores and repair batches; a batch ending in `DENIED` keeps that priority and clears
the size hint. Profile client publish, replica put and local stores also report their checked
payload bound; send and accept constants are separate and both 1 MiB today.
Search publication reports its local encoded-record size bound and, for insufficient profile
PoW only, the required difficulty. Gossip and catch-up use a separate fixed 64 KiB accept bound.
Media service rejections report the checked blob or signed-record bound: 6 MiB for client
uploads, 1 MiB for replica data and Keep/Attach authorizations. Node upload dispatch preserves
the upload hint; shard manifest validation retains its existing `INVALID` result.
Client retries and validation of reported values are also planned.

### Clock hints

Additive `uint64 server_time` fields carry the node's Unix time in seconds:

| Message | Tag | Intended use |
|---------|-----|--------------|
| `NodeInfo` | 10 | Node clock from `node.info` |
| `ProfileResponse` | 5 | Signed-time `STATUS_INVALID` |
| `MediaResponse` | 6 | Signed-time `STATUS_INVALID` |
| `SearchResponse` | 10 | Signed-time `STATUS_INVALID` |

Zero means no hint, including messages from older nodes. Prior tags, including size and PoW
hints, are unchanged; legacy decoders ignore the added fields. Fixed-wire tests in
`rust/bootstrap/tests/compatibility.rs` cover both directions. The node fills `NodeInfo` on
`info` and uses the validation clock snapshot for signed-time `INVALID` replies: profile/search
heartbeats, direct media keep/attach, and replica/shard authorizations (future skew or existing
retention expiry). Invalid signatures, versions, hashes and other failures leave the hint zero.
Existing freshness rules are preserved; this field does not introduce a new validation window.
Client correction using a median from three distinct nodes and device-clock guidance remain planned.

## FFI boundary

Protobuf types do not cross the FFI. `rust/ffi` exports UniFFI types; see the
[FFI contract](ffi-design.md).

## Network namespaces

`dyapp-p2p-net::Protocols` can derive namespaced protocol IDs and search topics from a
validated network name ([P2P networking](p2p-networking.md#protocol-namespaces)).
Protobuf packages, schemas and field numbers are shared across networks. Configured swarm
builders and node services use the same `Protocols` for negotiation, codec limits, role checks
and gossip topics. Different namespaces cannot negotiate the application RPCs or gossip,
although the common transport can connect. Binary network selection, storage guards and
foreign-peer disconnect policy remain planned; `dyappd` and legacy constructors still use the
public IDs.

## Evolution rules (proto3)

- **No versions.** Protocol IDs and packages carry no version. Messages change only by new
  fields and new `oneof` variants.
- **Field numbers are permanent.** A field can be renamed but never renumbered or retyped. When
  removing a field, add `reserved <tag>;` so the number is never reused.
- **Defaults are indistinguishable from absence** for scalar fields (`0`, `""`, `false`). Use
  `optional` when "unset" must differ from "false", as for `has_kids`.
- **Unknown fields.** An older node parses a message with new fields without error but does not
  understand them. Signed payloads are stored as received, so the fields survive. An unknown
  `oneof` variant decodes as empty and is answered `STATUS_UNSUPPORTED`; the client tries
  another node.
- **Type changes** are breaking: use a new field number and reserve the old one.
- **Packages are part of the type name**: `dyapp.profile.Profile` and any other `Profile` are
  unrelated types on the wire.

Use this checklist for each change class:

| Class | Compatibility checks |
|-------|----------------------|
| 1. Profile composition | Add fields with permanent tags and safe defaults; preserve signed payload bytes, including unknown fields. A new attribute still needs client UI support. Editing/re-encoding while retaining unknown fields remains planned. |
| 2. Size limits | Check the exact object unit (payload, encoded signed record or blob); keep client-send separate from network acceptance and wire framing; report the checked bound in `max_bytes`. Follow the [two-release raise/lower rule](bootstrap.md#limits-and-abuse). |
| 3. PoW difficulty | Report `required_pow_bits` only for an insufficient advertised difficulty; malformed content, signature failures and clock failures have different meanings. Search publish and heartbeat report it today; background client recomputation remains planned. |
| 4. Media replication | The node chooses replication and layout; clients upload once. Keep old manifests readable when changing R or parity. Current readers require exactly K+4 hashes, so flexible parity is planned. Changing the 1 MiB K unit first needs an additive `shard_size` field defaulting to 1 MiB, then readers before writers. |

Clients treat hints as untrusted ([ADR 0016](../decisions/0016-protocol-evolution.md)): zero
means absent, including older nodes. `dyapp_p2p_net::hints::Limits::with_hints` keeps the
client's compiled floors, lets a hint raise PoW up to 20 bits and lower sizes, and skips a node
whose profile size hint is below 64 KiB, media hint below 256 KiB or PoW above 20 bits. No client
calls it yet; shrink/retry once and background PoW recomputation remain planned. A change that
tightens security is a new `oneof` variant, not a new field; `NodeInfo.capabilities` lists
optional features, and unknown values are ignored.
