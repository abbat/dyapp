# Bootstrap Server Architecture (Distributed Relay)

## Overview

A bootstrap node (`dyappd`) serves owner-signed profiles and per-device mailboxes over
libp2p ([Served protocol](#served-protocol)), pushes new envelopes to watching devices and
repairs mailbox, profile and media replicas node to node; [profile search](#search)
runs as its own role, on by default, and signaling is planned.

> ⚠️ The server stores whatever bytes clients send as envelope ciphertext; no client encrypts
> yet. Profile writes need the owner's signature; mailbox reads and deletes need the device's.
> See [Encryption & Security Status](../security/encryption.md).

**Key principles:**
- **Replication**: acceptors replicate messages, profiles and media blobs up to 1 MiB to five DHT points, confirming two distinct holders. Nodes forward mailbox acks and repair mailboxes, profiles and media. Media up to 6 MiB uses Reed-Solomon above the operator's whole-copy threshold ([Replication and repair](#replication-and-repair), [Erasure coding](#erasure-coding-reed-solomon)).
- **Mailboxes**: one per device, read and emptied only with the device key's signature; envelopes expire after a TTL (default 24h, deleted hourly; see [Privacy](../security/privacy.md#retention))
- **Profile storage**: public profiles signed by the owner's identity key; the highest version wins and deletion is a signed tombstone ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); searched through the [search role](#search)
- **Rate limiting**: per-peer token bucket on profile and mailbox requests (see [Rate Limiting](#rate-limiting))
- **Encryption (target)**: clients end-to-end encrypt messages and media before upload; profiles are public and signed, not encrypted ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)); not implemented
- **Network**: open, anyone may run a node, Kademlia DHT discovery ([ADR 0007](../decisions/0007-open-bootstrap-network.md), [P2P networking](p2p-networking.md)); storage is a cache with an operator-set retention TTL (default 30 days) and eviction by profile activity ([ADR 0009](../decisions/0009-message-delivery-and-storage.md))

## Target Design — planned

Served today ([Served protocol](#served-protocol)): `/dyapp/node`, `/dyapp/profile`,
`/dyapp/keypackage`, `/dyapp/mailbox`, `/dyapp/mailbox-push`, `/dyapp/media`, `/dyapp/search`, `/dyapp/inventory` and
`/dyapp/digest`,
the store, media and TURN roles, the search role (gossipsub, `/dyapp/search-kad`,
`profiles-idx.db`), mailbox replicas and repair over the DHT, signals in the mailbox, ranged media reads with the assembly cache. The search index rebuild, search defences against fake-profile and query
floods ([search capacity](search-capacity.md#adversary)) and the store format path are planned; each part below says which it is, and the sections after
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
- **Clients upload once, nodes replicate.** The acceptor looks up the five holders of a
  mailbox, profile or media blob up to 1 MiB, writes each distinct holder, and waits for two
  stored copies. Sharded media confirms two manifests and K shards.
- **Nodes see metadata, not content.** Messages, likes, views and every other signal are
  end-to-end encrypted for the recipient. Nodes see which mailbox gets mail and when; sealed
  sender signs each envelope with a fresh key, so they do not see who wrote it.

### Roles and discovery

Each role is enabled separately in the node config:

| Role | Holds |
|------|-------|
| store | signed profiles, device KeyPackages, per-device mailboxes (messages, likes, views and other signals) |
| media | media blobs and erasure-coded shards, thumbnails |
| search | the search index over profiles collected from the whole network |
| TURN | short-lived credentials for an operator-run coturn relay of call media |

Every role has its own DHT key space, so replicas for a role are chosen only among the nodes
that run it. A node announces its roles through libp2p identify. A reachable node is a DHT
server; a node behind NAT is a bootstrap only for peers in its local network. Mobile clients run
the DHT in client mode: they store nothing and answer no DHT queries. Today `/dyapp/kad` is the
store role's key space: `dyappd` without the store role runs it in client mode and serves no
protocol. The media role needs the store role and shares its key space for now. The search role
runs its own Kademlia, `/dyapp/search-kad`, besides the main one ([Search](#search)).

Role model for the first release: `roles` is an explicit list, the default `["store", "search"]`.
`store` stays an explicit base role, refused at startup when another role lacks it: it selects
DHT server mode and every other role uses its key space, so a node without it serves nothing, and
an implicit store would hide what a node writes to disk. `search` stays on by default so an early
network with a few nodes has search at all: every search node keeps the whole index (up to
`search.max_bytes`, default 50 GiB, in `profiles-idx.db`), checks proof of work and pays the
catch-up and inventory traffic from `maintenance.io_mb_per_s`. An operator with a small disk lowers
`search.max_bytes` or lists `roles = ["store"]`. Whether search becomes opt-in is decided once
search coverage is measured at real node counts; existing configs keep working either way.

Kademlia gives every node an equal share of keys, and v1 does not weight it by capacity: a weak
node refuses or evicts what exceeds its limits and the other replicas keep the data. Several
virtual positions per node would each need a proof of work and would open a Sybil gap.

### Protocol

Address admission is being tightened. The shared `same_ip` helper compares exact leading
IPv4 or IPv6 addresses independently of ports and transport; it rejects DNS, relay/circuit
routes, missing IPs and embedded or multiple IP components. The bounded state for probing
claimed addresses exists (`claims` in dyapp-bootstrap): at most 4 queued or dialled
addresses per peer and 64 in total, an address pending for any peer is not queued again, one
dial per peer at a time with its connection ID, an address dialled at most once an hour, and
the 4096 most recently dialled addresses remembered. Only a connection of the claiming peer
on that dial's ID proves an address; a failed or at once refused dial frees its slot, and a
peer's queued claims go with its last connection. The node now uses `same_ip` for identify listen
addresses on the connected peer's exact remote IP and probes other claimed addresses with
one-address, peer-ID-pinned dials before routing to them. Failed, stale and unrelated connections
prove nothing. Both DHTs filter inbound records: no remote value records are stored, and
search Kademlia stores no remote providers. The store DHT keeps a provider record only for
`dyapp/search` and `/dyapp/turn`, from a peer ID with the node-ID proof of work, and only when
every advertised direct address has the exact IP of a live connection to that provider,
regardless of the address's port or transport; a disconnected provider or a forged foreign
address is rejected. Local provider announcements still work. A search node dials found
providers only when their peer ID carries the proof of work. The outbound dial guard is
described in [P2P networking](p2p-networking.md#dial-filtering).

One libp2p request-response protocol per service, protobuf requests and replies, schemas in
`proto/`. `proto/node.proto` defines `/dyapp/node`, `/dyapp/profile`, `/dyapp/keypackage`, `/dyapp/mailbox`,
`/dyapp/mailbox-push`, `/dyapp/media`, `/dyapp/search`, `/dyapp/inventory` and `/dyapp/digest`
([schema](protobuf-schema.md#node-protocol)); the other services get their
schemas with their roles. Each request is a `oneof`; a node that gets a variant it does not know answers
`unsupported` and the client tries another node. Every reply carries a status: `ok`, `not_found`,
`stale`, `too_large`, `full`, `rate_limited`, `denied`, `unsupported`, `invalid`, `refused`.

| Protocol | Role | Requests |
|----------|------|----------|
| `/dyapp/node` | all | `info`: roles, limits (payload, message, mailbox, media), supported search filters, minimum profile proof of work, retention TTL |
| `/dyapp/profile` | store | `publish(SignedRecord)`, `get(identity)`, `heartbeat(SignedRecord)`; node to node `replica_put(SignedRecord)`, `inventory(identity)` → version |
| `/dyapp/keypackage` | store | `publish(device-signed KeyPackageRecord)`, `fetch(owner, mailbox, profile version, slot)`; node to node `replica_put` |
| `/dyapp/mailbox` | store | `challenge`, `put(envelope)`, `fetch(mailbox)`, `ack(ids)`, `revoke(token keys, or close)`; node to node `replica_ack`, `replica_revoke`, `inventory(ids)` → `missing(ids)`, `replica_put(envelopes)` |
| `/dyapp/mailbox-push` | client | the node pushes new envelopes to a connected device over its connection |
| `/dyapp/media` | media | `keep(signed hash list)`, `put(owner, blob)`, `get(hash)`, `get(hash, offset, len)` for a piece, `attach(signed chat attachment)`, `release(secret)` |
| `/dyapp/search` | search | `publish(SignedRecord)`, `heartbeat(SignedRecord)`; `query(conditions, limit)` → (identity key, version) pairs in random order, the conditions applied, `partial`; `get(keys)` → `SignedRecord`s |
| `/dyapp/inventory` | search | node to node: `(topic, after, before)` → up to 10 000 (key, version) pairs sorted by key, for search catch-up; `before` is an exclusive upper bound, empty = none |
| `/dyapp/digest` | search | node to node: `(topic)` → the 256 bucket hashes of the topic, `(topic, bucket)` → the 256 sub-bucket hashes of one bucket; 32 B each, same guards as `/dyapp/inventory` |
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
| `mailbox.revoke` | the mailbox's device key, over the request (no nonce: a replay revokes the same keys); to close the mailbox, the identity key the device accepted in its `owner` binding | mailbox address = H(device key), or H(the `owner` binding's signer) |
| `mailbox.put` | a sender key, over the envelope (fresh per envelope from `rust/client`) | per-key and per-IP-group quotas; an optional write token signed by the recipient device key ([Mailbox envelope](#mailbox-envelope)); the recipient checks the sender inside the MLS ciphertext |
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

Profiles, mailbox envelopes (signals included) and media blobs up to 1 MiB are replicated whole to R = 5 points,
replica *i* on the nodes closest to H(key ‖ i); media above the node's threshold are
erasure-coded into K = ⌈size / 1 MiB⌉ data shards + M = 4 parity shards, up to 6 MiB.
Smaller blobs use whole copies ([Erasure coding](#erasure-coding-reed-solomon)).
`dyapp_p2p_net::replica_key(key, i)` is the Kademlia lookup key `key ‖ i` (Kademlia applies
SHA-256). A client sends one put to the holder of one replica key and falls back to the others on
any failure. It keeps local scores of nodes (`rust/client` `scores`): a refusal, a timeout,
invalid data or an older profile than another replica adds one to a node's penalty, which halves
every day, and `scores::first` tries the lowest penalty first. A node's own `info` and status are
hints only and never raise its score. The scores live in memory until the app stores client state.
The acceptor verifies and bills profile/mailbox writes five token-bucket units, runs a full DHT lookup of every
replica key, stores locally only when it holds a key, and sends a signed `replica_put` to each
other distinct holder. Receivers verify records independently and never forward replica puts.

Mailbox/profile fan-out, profile-gap repair and media replica/shard writes (including repair)
share one sender token bucket per destination: average 50 write units/s, burst 50. A refused send
is dropped without queuing or lowering the destination's trust; a local write bypasses the bucket.
`replica_dropped` counts refused send operations since the preceding hourly status line, and taking the count does not
refill the bucket. Mailbox repair uses the same budget, splitting at most 1 MiB of selected
records into batches of at most 50 envelopes and charging one unit per envelope. Only admitted
batches are sent; refused batches are dropped and counted once per batch. Subsequent inventories
omit envelopes already received, so larger gaps can make progress after refill. Media writes
cost one unit each when a stream slot is available; refused ready jobs leave the media queue
without an outbound request or remote-failure penalty. Media reads and inventories use no
replica units; local media writes bypass the cap.

The reply waits for two distinct holders to store. Several replica keys on one node count once.
An isolated node with no routing peers may acknowledge one local copy; a node with peers must
confirm two. At most 64 fan-outs are active, each for 10 seconds; excess or timed-out writes answer
`FULL`, so the client tries another key. Other holders are filled in the background after the
reply, without a persistent queue. A profile holder with an equal or newer verified record
counts as stored and supplies the highest confirmed stale record in a `STALE` reply.
The app clients do not call this protocol yet; `dyappd put` exercises the single-upload path.

Media keys use the blob's SHA-256. The acceptor carries a persisted signed keep or attachment
in `MediaReplicaPut`; a receiver independently verifies it and never forwards it. A keep is
stored only when newer, and the hash must occur in the latest stored keep: an old proof cannot
restore a removed hash. Replica keep timestamps may be older than ten minutes, but must be
within the owner TTL and no more than ten minutes ahead; replay never refreshes liveness.
Attachments retain their original signed creation time and expiry. Legacy lists without a
persisted signature require a fresh signed keep before upload. A valid unexpired attachment
is preferred when both authorizations exist.

Whole-copy media puts cost payload bytes × 5 against the peer byte bucket and owner quota, including
repeated puts and deduplicated copies. Gets cost reply payload bytes. Node replica puts charge
one payload against the sending peer's byte bucket. The byte bucket uses `bytes_per_second`
(0 disables it), alongside the existing request limits. The acceptor reserves the additional
four payload copies in the shared 75 % traffic budget before forwarding; sharded puts reserve
ceil(bytes × (K+M)/K) minus the uploaded payload and use that same billed quota. Outbound framing is
counted separately. Physical storage deduplicates by hash, while every owner pays its quota.
The `node status` log line reports `media_replica_holders`, distinct confirmed holders of the most recent
media write, including late confirmations; this in-memory snapshot is not a durability census.
Media currently shares the store DHT: all selected holders must serve media, otherwise their
`UNSUPPORTED` replies cannot count toward confirmation. Keeps and releases are node-local;
release each holder separately. A holder keeps a released attachment's release hash until the
attachment's expiry and answers an `attach`, replica put or repair carrying it with `REFUSED`. A successful keep starts media repair; list forwarding without
missing data remains planned, so refresh keeps on their holders.
`dyapp_client::Node` makes the client calls: `keep_media`, `put_media`, `attach_media` for an
`Attachment`'s blob, `get_media` for a whole blob checked against its hash, and `release_media`.

Repair is driven by the owner's presence; mailbox, profile and media repair are implemented.
When a user comes online, the nodes
responsible for their keys compare inventories, *I have* and *I need*, and fill the gaps, so new
closest nodes get the data after churn and stale replicas catch up. Data of a user who stays
offline is not repaired and expires with the TTL; a message whose replicas are all lost is
resent from the sender's retry queue.

- **Profile.** A successful publish (including `STALE`) or owner-signed heartbeat makes the
  accepting node look up all five current replica keys and query each distinct trusted remote
  holder with `profile.inventory(identity)`. The reply carries only the stored version, or
  `NOT_FOUND`. Missing and older holders get the owner's verified record via `replica_put`,
  with `age_seconds`, the time since the owner was last seen here; the receiver sets its
  last-seen time that far back (a later time it already has stays) and drops a record whose age
  reaches its `limits.profile_ttl_days`. Equal or newer holders get no record. Replica puts do not start another repair.
  A node inventories a profile at most once per rolling hour, with up to 4096 keys tracked;
  the limit is in memory and starts over after a restart. Above the cap it waits for a later visit.
- **Media.** The repair engine can run without a client response channel; an accepted signed
  `keep` attaches one to receive the final missing list. Either way, a repair checks at most 256
  hashes from a rotating per-owner cursor. The node looks up the current blob and shard keys,
  groups their `MediaPart` entries by holder and sends one `have` batch per distinct holder.
  An absent index names a whole copy or manifest; a present index names a shard. Replies
  partition allowed entries into present/missing and include bounded manifests and whole-copy
  availability.
  A receiver answers only a trusted peer its routing table puts among the five nearest peers
  of a blob/shard replica key; other entries are omitted, not reported as present.
  The repairer uses its verified local data or fetches available whole copies or valid shards.
  It writes only missing whole copies, manifests and shard indices with the owner's signed keep.
  A reconstructed shard requires K valid indices and a matching final blob SHA-256; corrupt
  local manifest/shards are discarded, while ownership lists remain for reupload.
  No remaining copy, fewer than K indices, failed validation or incomplete checks leave the
  hash in the keep reply's `missing` list. Successful checks remove it even if the acceptor
  holds no blob key. Restored puts still obey the receiver's latest keep, TTL, quota and deny list.
  At most four keep repairs run, each for fifteen seconds, processing one payload at a time.
  A key is inventoried once per rolling hour, with up to 4096 keys and owner cursors tracked
  in memory; later visits reuse that result or wait for capacity. Successful uploads clear a
  cached missing result. Restarts reset the schedule. Repair reserves request and bounded reply
  bytes in the same 75 % traffic budget as assembly, with the shared two-per-peer/sixteen-node
  media queue. Timeout/budget failures may conservatively request a client reupload.
- **Mailbox.** A watching fetch makes the node the repairer for that mailbox: it sends the ids it
  holds to the closest node of each other replica key in a `mailbox.inventory` request. The
  peer answers the ids it lacks, and the repairer sends those envelopes in one `replica_put`,
  each verified like a put. The peer sends the envelopes the repairer lacks back only
  if its own routing table places the repairer among the closest nodes of one of the mailbox's
  replica keys, so an arbitrary peer cannot pull a mailbox's ciphertexts.
- **No grace period.** Nothing is copied when a node leaves or restarts; the owner's next
  visit or a holder's republish moves only the gaps, so a restart never moves the node's whole
  store.
- **Network growth.** When nodes join, the holder of some replica keys changes and the new
  holder has nothing yet; a read there misses and the client tries the next key. Repair checks
  the current holders, so a present owner's data moves to them. For an absent owner the holders
  republish: ten minutes after the first finished Kademlia bootstrap, and then hourly, a node
  scans up to 10 000 held keys per store (mailboxes, profiles, media keeps) and fingerprints the
  five closest peers its routing table gives for each replica key. A key whose fingerprint
  changed since its last complete handover is queued and handed over through the repair above:
  mailbox inventories, profile version inventories, media `keep` repairs. At most 16 keys are in
  flight, none start while traffic is at 50 % of `bytes_per_second` or above, and a key the
  owner repaired this hour is skipped. A handover that is cut or fails leaves the key queued
  for the next scan. The old holder keeps its copy until the TTL. Restarts reset the
  fingerprints, so the first scan after a restart inventories every held key once.
- **Budget.** One inventory per mailbox per hour on a node; profile repair uses a rolling
  one-hour limit. Repair traffic counts towards `bytes_per_second` and stops at 75 % of it,
  together with media. Profile inventory requests and gap transfers reserve their encoded
  bytes before sending, so a large record cannot overshoot the remaining repair budget.
  Skipped work waits for the owner's next eligible visit.
- **Scale.** 120 million users on 1200 nodes with R = 5 put about 500 000 users and 750 000
  device mailboxes (1.5 devices per user) on a node. If half the devices come online daily, a
  node sends about 4 inventories a second to 4 peers each, a few hundred bytes apiece (16-byte
  ids of pending messages); envelopes move only for real gaps.

### Profiles

- Payload at most 1 MiB today, a node send/accept value reported in `info`
  ([Limits and abuse](#limits-and-abuse)). Media are separate blobs; the profile only links to
  them by content hash.
- Each photo in `photos` is the full image (one or more blobs) plus a separate thumbnail blob,
  stored whole: a JPEG, long side at most 320 px, at most 24 KiB (`make_preview` and
  `check_preview` in `rust/ffi`). Lists and search results load only thumbnails; the full image
  loads when a profile is opened (implemented in the schema and checks; no client yet).
- Location is `place`, the decimal GeoNames ID of a city or district, never coordinates
  ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).
- **Canonical places.** The node embeds the GeoNames cities1000 set (about 171k places, CC BY
  4.0, see `NOTICE`) in `rust/bootstrap/data/places.bin`, about 190 KB: one block per country
  with its sorted IDs as LEB128 deltas. A lookup skips to the country's block and decodes it,
  at most a few tens of thousands of IDs. A release refreshes the file with
  `tools/scripts/geonames-places.py`. The search index stores a place outside the set, or an empty
  one, as `""`: such a profile is found by its country only.
- Every profile string follows one rule, `dyapp_profile::is_clean_text`: NFC, no control,
  format (bidi, zero-width) or private-use characters except ZWNJ and ZWJ; enum-like strings hold
  at most 64 characters and `interests` at most 32 items of 64; `publish` refuses a profile that
  breaks it ([ADR 0026](../decisions/0026-profile-text-rules.md),
  [Threat Model](../security/threat-model.md#spoofing)). The optional public `name` follows the
  same rule as an enum-like string; it proves nothing, so clients show it with the verified or
  unverified mark and a contact's local name replaces it
  ([ADR 0027](../decisions/0027-optional-public-profile-name.md)).
- The identity-signed profile can carry a validated device directory. The node's KeyPackage
  rendezvous and the sender workflow are still planned. The directory is public, including the
  device count. See [Device directory and KeyPackages](#device-directory-and-keypackages).
- The profile carries a proof of work bound to the owner's key (
  [ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)): fields `pow_nonce` and
  `pow_bits`, valid when Argon2id(salt `dyapp-profile-pow`, password signer key ‖ nonce, 8 MiB,
  1 pass, 1 lane) starts with `pow_bits` zero bits. It does not depend on the profile's content,
  so it is computed once, about 30 s at 11 bits on a phone. The client computes it in the
  background, keeps the last tried nonce to resume after the app is suspended, and publishes to
  store and search only when it is done; messaging works before that. Both roles reject a
  profile below their shared minimum or with a forged proof. Only zero-minimum test networks
  accept a zero-difficulty profile.
- Deletion is a signed tombstone kept for the TTL
  ([ADR 0011](../decisions/0011-best-effort-deletion.md)).

### Device directory and KeyPackages

**The profile field, certificate checks and node KeyPackage protocol are implemented; the
client workflow is planned.** [ADR 0019](../decisions/0019-device-directory-and-keypackage-rendezvous.md)
fixes the format. `Profile.devices` (field 24) holds at most 64 entries sorted by mailbox address
and no duplicate keys. Each entry is an
identity-signed `DeviceCertificate` containing the 32-byte Ed25519 device public key, the distinct
32-byte MLS signature public key, and a device generation. The enclosing profile signature and
strictly increasing version make the entire list authoritative. A device's mailbox is
SHA-256(device public key). A KeyPackage's MLS leaf signature key must equal the certified MLS
key. A sender fetches at least two profile replicas, retains the highest version seen for each
contact, and refuses a lower version, any version after a tombstone, or a device absent from
that directory (`rust/client`: `profiles::newest` over the replicas, `profiles::Profiles::accept`).
A node that served a lower version loses score ([client scores](#replication-and-repair)). If the directory is
unavailable, first contact waits; no guessed mailbox or stale package is used.

`/dyapp/keypackage` on store nodes accepts a device-signed publication and an unauthenticated
fetch. The publication binds the owner, mailbox, exact MLS KeyPackage bytes, expiry (at most 30
days), `last_resort` bit and rotation revision. The node verifies the current identity-signed
profile and device certificate, outer device signature, size, expiry and MLS suite header before
accepting a publication, then stores it at R = 5 owner-derived replica points alongside the
profile. A client publishes to a holder that has already stored its current profile; another
node rejects the record until it receives that profile. The node does not parse or verify the MLS
leaf signature, certified MLS key or MLS lifetime; clients must do so
before use. A fetch names the owner, mailbox, profile version and one slot (0–4). The node returns
only packages for a device in a profile at least that new. The sender independently checks the
certificate, MLS signature/key, lifetime, expiry and suite against its newest profile.

Each ordinary package belongs to one slot, assigned by the first byte of its hash modulo 5. A
fetch for that slot is serialized locally: in one durable transaction the responding replica marks
one package consumed before replying; replayed fetches cannot retrieve it there. Other replicas
can independently allocate their copies, so there is no cross-node single-use guarantee.
The sender retries a fresh package on a
failed Welcome; the recipient rejects a replayed group or duplicate Welcome. The reusable
last-resort package is returned only when no ordinary package can be obtained and is clearly
marked. Reuse permits replay attempts and weaker first-message forward secrecy, so the client
prefers a fresh package and rotates the last-resort one when online
([RFC 9420 §16.8](https://www.rfc-editor.org/rfc/rfc9420#section-16.8)).

A device publishes 20 ordinary packages (four per slot) and one last-resort package, refilling
when fewer than eight ordinary packages remain or during its daily heartbeat. The node limits
ordinary stock to 20 and last-resort stock to one per device, each signed publication to 16 KiB,
and total KeyPackage storage to 512 KiB per device; it rate-limits unauthenticated fetches by
peer ID and IP group, including fetches that consume a package. Packages expire within 30 days;
no package is silently extended by a profile heartbeat. If all ordinary packages are gone or
holders are unreachable, use the last-resort package; if it too is absent, first contact waits
for refill. Node eviction and malicious withholding remain denial-of-service risks.

Removing a device means publishing a higher-version profile without its certificate. Nodes
refuse new package publications for it and stop serving its packages after they learn the new
version; clients reject its packages and remove it from MLS groups as soon as they learn the new
profile. The old mailbox and already delivered ciphertext cannot be erased reliably. A stale
profile withheld by every queried replica can delay revocation, so a verified contact is shown
as changed and requires renewed verification on a device-list change. The public directory and
fetch reveal device count and contact interest to store nodes; no private directory is promised.

### Search

- **Discovery.** Once its routing table has a peer, a search node announces itself as a
  Kademlia provider of `dyapp/search` in the main `/dyapp/kad`, like the TURN role; kad
  republishes the record every 12 hours. Clients and new search nodes find search nodes with
  `get_providers` on that key; the client's `dyapp_client::Node::providers` dials each provider
  once while the lookup runs, because kad keeps a provider's addresses only for the query, and
  returns the addresses for `Node::connect`. A peer that lists `/dyapp/search-kad` in identify joins the
  node's search Kademlia, which links the search nodes to each other, under the same node-ID
  proof of work and IP-group limits as the main routing table
  ([routing defences](p2p-networking.md)).
- **Dissemination.** Clients do not run gossipsub. On every change the owner's client sends its
  `SignedRecord` to 2 search nodes with `/dyapp/search` `publish`, and once a day a signed
  `heartbeat` (key, profile version, time). The entry node checks the signature, the size, the
  version and the per-key limit, stores the record and publishes the request as received to the
  gossipsub topic `/dyapp/profiles/<n>`, n = H(key) mod 16. A node whose `search.topics` leaves
  out a key's topic does not store it, but as an entry node still publishes it and answers `OK`.
  The mesh holds search nodes only, not store nodes or phones. Gossipsub uses its default degree
  (D = 6), a message id of SHA-256 of the message bytes and manual validation before relaying;
  a forged or undecodable message is a strike against the peer that relayed it in the node's own
  ban score, gossipsub scoring is not used. Gossip accepts signed records up to 64 KiB,
  plus request-wrapper overhead.
- **Per-key limits.** At most 1 profile update per 10 min and 2 heartbeats per day per key. The
  entry node answers an excess publish `RATE_LIMITED` with `retry_after` in seconds and never
  drops it silently; gossip relays drop extras silently. The client remembers when it last
  published to search. Within the 10 min it holds the latest profile, merges further edits into
  it, sends it when the window ends (or on the next app start if the app was closed) and tells
  the user "changes appear in search in N min". The profile in the store DHT is updated at once,
  without this limit.
- **Proof of work.** Store and search share `search.min_profile_pow_bits` (public default 11,
  at most 20), advertised by every store node in `info.min_profile_pow`. A private test network
  may explicitly set it to 0 on all nodes; changing `network.name` alone does not lower it.
  Store and search admit only profiles whose `pow_bits` reach that minimum; a lower
  claim is `INVALID` with `required_pow_bits` and no strike. The claim is checked with
  Argon2id (above, [Profiles](#profiles)) off the swarm loop on its own permits (half the cores,
  at least two), so checks cannot starve queries: a client publish or heartbeat that finds them
  all busy gets `RATE_LIMITED` with
  `retry_after` 1, a gossip message waits in a queue of at most 64 and is ignored without a
  strike when it is full, and a catch-up reply waits for a permit as one job. A forged claim is
  `DENIED` on publish and rejected in gossip, both a strike against the sending peer. Only
  confirmed (key, nonce, bits) triples are cached, so Argon2id runs once per key and a forger
  cannot fill the cache. A tombstone keeps the proof fields, so it is checked like the profile.
  Store client and replica writes share the bounded proof workers; a busy worker returns
  `RATE_LIMITED` without a strike. A local holder accepts only a cached proof and returns
  `RATE_LIMITED` on a cache miss, so Argon2 never runs on the swarm loop. Zero-bit proofs need no
  hash on a zero-minimum test network; positive claims are still checked. Search heartbeats also
  check the difficulty of the stored profile:
  below the current minimum they return `INVALID` with `required_pow_bits`, without changing
  heartbeat time or daily counters; gossip ignores them without a strike. Raising the minimum
  does not purge indexed profiles, so their last accepted heartbeat leaves the usual retention
  window (default 30 days) to recompute and republish. Client background recomputation is planned.
- **Heartbeat proof of work.** While `search.min_heartbeat_pow_bits` (default 8, at most 16,
  0 = off, reported in `info` as `min_heartbeat_pow`) is above 0, a search heartbeat carries a
  daily Argon2id proof (`pow_nonce`, `pow_bits`, `pow_day`; same parameters as the profile
  proof, salt `dyapp-heartbeat-pow`, password key ‖ day ‖ nonce), so keeping a fake profile alive
  costs a fresh proof every UTC day. Checks run cheapest first: signature, clock, topic, stored
  profile difficulty, then the stored version and the daily limit without counting the beat
  (`NOT_FOUND`, `RATE_LIMITED`), so a replayed or surplus heartbeat never costs Argon2id. A proof
  below the minimum or for a day other than the node's current UTC day is `INVALID` without a
  strike; a forged one is `DENIED` (gossip: rejected), a strike. Confirmed (key, nonce, day)
  proofs are cached like profile proofs, so the second heartbeat of the day costs nothing.
  Client-side solving is planned.
- **Index.** A search node subscribes to all 16 topics by default, or to the part listed in
  `search.topics`, and then holds a uniform sample. A subset is refused at startup on the public
  network (empty `network.name`), so a public search node answers for every topic. The index
  is its own disposable file, `profiles-idx.db`, which also keeps the signed records in a
  `record` table apart from the store's profiles; catch-up refills a lost or dropped file
  ([rebuild](#storage-on-a-node)). The search role needs the store role, since a
  node without it serves no protocol. It holds typed columns for every
  searchable `Profile` field (`search_fields`: all but `name`, devices and proof of work; photos only as their count), plus key, version, heartbeat time and a random u64 `r`, and a
  table of (key, interest) pairs. `r` is drawn again for every new version. Indexes are (country, place, r), (country, r)
  and (r). A `place_count` table keeps the number of indexed profiles per (country, place),
  updated on every put and removal; `key` also keeps `first_seen` (the first put, never
  changed) and `bytes` (the signed record's size, 0 for a tombstone), and `meta` keeps the
  file's `created` time. Client publication checks the encoded signed record against `search.max_profile_bytes`
  (1–65536 bytes, default 64 KiB, reported in `info`). Gossip and inventory catch-up instead
  accept records up to the fixed 64 KiB network bound, even when the local send limit is lower. Storage is bounded by
  `search.max_bytes`, records and index together, checked on every put. Past it the node evicts
  established keys from the place with the most profiles (`place_count`, trying the 16 largest),
  highest `r` first, else any established key by `r`; tombstones are never evicted by size. A key
  is a newcomer for `search.newcomer_days` (1–365, default 30) after its `first_seen`; newcomers
  hold at most `search.newcomer_share` (in (0, 1], default 0.1) of `search.max_bytes`, and a put
  past that quota evicts other newcomers, oldest heartbeat first. Keys received by inventory
  catch-up while the file is younger than `search.newcomer_days` are stored as established, so a
  new node fills from its neighbours. Every profile also leaves when no heartbeat came within
  `limits.profile_ttl_days`: the hourly cleanup removes at most 4096 such keys per run. A new indexed field bumps the index version; the node builds the new
  generation next to the old one, keeps answering from the old one and switches when the new one
  is ready ([generations](#storage-on-a-node)). Version 2 added `gender`; version 3 replaced `education` with `education_level`; version 4 added the photo count.
  A derived `digest` table stores, per (topic, bucket), the XOR of
  SHA-256(key || version as eight big-endian bytes), including tombstones. Each topic has
  256 contiguous buckets and each bucket 256 sub-buckets, selected by the first 16 key bits
  after the topic nibble. Puts, overwrites, tombstones, expiry and eviction update bucket hashes
  in the same transaction as `key`; heartbeats do not change them. Sub-bucket hashes are
  computed with a range scan and report the scanned row count. Every open rebuilds the derived
  table from `key` and logs rows and elapsed milliseconds; this full scan adds startup time
  proportional to the number of keys. `/dyapp/digest` serves these hashes and
  `/dyapp/inventory` takes an exclusive upper bound; a search peer pays for a bucket digest out
  of its sync budget as the larger of the reply and 40 B per scanned row, so a digest costs as
  much as the inventory of the same rows.
- **Catch-up.** A minute after start and then hourly, unless the node sheds load or the previous
  catch-up still has digest or inventory requests waiting or in flight (the tick is skipped with
  a debug log), it picks two random search nodes of its gossip mesh for each kept topic (one if
  the mesh has one), so a single peer that keeps a subset or withholds records leaves no silent
  hole, and compares the topic with each. It asks `/dyapp/digest` for the 256 bucket hashes, then
  for each differing bucket its 256 sub-bucket hashes, and pages `/dyapp/inventory` with
  `after`/`before` over each run of adjacent differing sub-buckets. More than 64 differing
  buckets fetch the whole topic, more than 32 runs of differing sub-buckets the whole bucket.
  Any digest failure (unsupported, timeout, an error status, not 256 × 32 B) falls back to the
  inventory of the whole topic or of that bucket. At most 8 digest and inventory requests are in
  flight; asks queued for a peer are dropped when its last connection closes. A page returns up
  to 10 000 (key, version) pairs sorted by key, and the node asks again from the last key while
  a page comes back full; it drops keys at or past `before` and stops paging that range when it
  drops one, for a peer that ignores the bound. Keys that change mid-way are picked up next hour
  or by gossip. A peer in sync costs about 8 KB per topic digest, and each differing bucket
  8 KB more plus the inventory of its differing ranges, instead of the whole topic inventory.
  Answers are unverified as before: a false "all differ" costs at most the old full inventory, a
  false "all equal" skips that peer for an hour. A node that keeps a partial sample differs in
  many buckets and pays about the old cost. The keys it lacks or holds older it
  fetches with `get`, 50 keys and at most 4 requests in flight per peer, paced by the shared
  `maintenance.io_mb_per_s` budget, and stores them without the 10-minute limit. A search node
  pays for these gets and inventories out of its own budget of `maintenance.io_mb_per_s` reply
  bytes per second instead of request units.
- **Conditions.** A condition is (`Profile` field number, op, values), at most 16 per query and
  one per field. Ops: `eq`; `in` with up to 8 values (for interests: any of them); `range`,
  inclusive, for numbers. Values are int64 or strings; strings match exactly. Income "at least X"
  is `income_to ≥ X`. `country` is optional. A node skips a field or op it does not know and does
  not list it as applied, so the client filters the rest; a value of the wrong type is
  `INVALID`. `search_results::Filter` builds them from what a person picks (age from 18,
  genders, orientations, goals, children, least education level, with a photo, active since a
  day, country, places and income, the last two only with a country); income matches when the
  profile's range overlaps the wanted one
  (`income_from ≤ max`, `income_to ≥ min`). Field 1000 (`search_results::ACTIVE_DAY`, listed in
  `search_fields`) is no `Profile` field: it is the UTC day of the key's `heartbeat` time, its
  last heartbeat or the time this node stored its profile, also from gossip or catch-up. The
  client cannot re-check it, so it drops the replies of a node that did not apply it.
- **Queries.** A query is unsigned. The node draws a random `s` and runs
  `WHERE conditions AND r ≥ s ORDER BY r LIMIT n`, wrapping around to the start if fewer rows
  come back, and scans at most `search.max_scan_rows` (10 000) rows, otherwise it sets `partial`;
  `partial` is also set while the index refills after a forced drop.
  The reply holds (identity key, version) pairs, 40 bytes each, 50 by default and at most 200,
  with the conditions applied, `partial` and the active generation's index version. A repeated query gives a new
  sample; there is no pagination. The client fetches the records it does not have in its cache
  with `get(keys)`, up to 50 keys, from the same node; the reply is cut at 1 MiB and the client
  asks again for the rest. A `query` costs 1 unit and a `get` one unit per started 64 KiB of its
  reply, limited per peer ID and IP group; scraping public profiles is accepted. Queries and gets
  run off the swarm loop, at most 2 at a time, from a queue fair between IP groups
  ([Rate Limiting](#rate-limiting)); a read the queue refuses gets `RATE_LIMITED` with
  `retry_after` 1.
- **Load puzzle.** Every 10 s the node takes the median wait of the reads it started from the
  queue: above `search.puzzle_wait_ms` (default 100) the puzzle turns on at 12 bits and then
  climbs one bit per tick up to `search.max_puzzle_bits` (default 20, 12 to 32); below half of it
  it falls one bit per tick, and off from 12. While on, a client's `query` or `get` needs
  `puzzle { challenge, nonce }` where the first 128 bits of
  `sha256("dyapp-search-puzzle" ‖ challenge ‖ peer ID ‖ nonce as u64 big-endian)` have at least
  `puzzle_bits` leading zeros; without a valid one the reply is `PUZZLE` with
  `puzzle_challenge` and `puzzle_bits`, and no strike. `info` reports both too. The 16-byte
  challenge rotates every 60 s and the previous one is still accepted; a solution is used up
  when the request enters the queue, so one refused there still counts on retry. Connected
  search nodes are never asked. The client (`dyapp_client::Node::search_query` and
  `search_get`) solves it with `dyapp_p2p_net::puzzle::solve` and resends once; it skips a node
  asking more than `MAX_PUZZLE_BITS` (24).
- **Results are a sample.** A reply is the node's local sample: possibly partial (a warming or
  evicted index, gossip it missed, `partial` set or not) and untrusted, since any operator can
  run a search node and omit or reorder hits. An empty reply does not mean nothing matches. The
  client queries several independently discovered search nodes (at least two) and passes the
  `get` records of all replies to `dyapp_p2p_net::search_results::check`. It keeps one profile
  per owner, the highest validly signed version, so a node cannot bring back an older version;
  drops it when it is a tombstone, misses a condition, or its profile proof of work is below the
  client's minimum or not confirmed by the Argon2 hash; and skips the conditions a node skips
  (unknown field or op, `range` on a string field). `place` is compared as the profile states
  it. `dyapp_client::search::search` runs this over the given nodes: it fetches only the hits
  whose version the client's profile cache lacks, in gets of 50 keys, asks again for the keys a
  `partial` reply left out, skips a failing node and returns the profiles in no set order.
  Replies carry no coverage hints.
- **Capacity.** [search-capacity.md](search-capacity.md) checks this design up to 50M users: it
  holds once 2 search nodes exist. An `in` on `place` runs as one ordered scan per place merged
  by `r`: a single SQLite query sorts every row of the places in a temporary B-tree and
  escapes `max_scan_rows`.

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
- Chat attachments are encrypted media blobs: a fresh key per attachment
  ([Encryption](../security/encryption.md#target-design); `Session::send_attachment` sends them). The sender attaches them under the SHA-256 of a
  release secret of 32 random bytes and sends the secret inside the message. Recipients do not
  release them, since every device of a chat is an MLS member and the first release would drop the
  blob for the others; attachments expire after an operator-set retention.

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
  reported once. Store format rebuild workers remain planned; the search index rebuild is
  implemented. Every index mutation (put, tombstone, heartbeat, expiry, eviction) already marks its key, after
  the commit in the same writer lock hold, while tracking is on (`track_start`, `take_dirty`,
  `track_stop` in `rust/bootstrap/src/search/writer.rs`); past 262 144 keys (about 16 MiB) the
  set is dropped and `take_dirty` returns `Overflow` until tracking restarts. The search
  candidate build (`build` and `replay` in `rust/bootstrap/src/search/candidate.rs`) starts
  tracking, then copies the index into `profiles-idx-v{N}.db` in key order, 256 keys per batch:
  key rows and tombstones as they are, `profile` and `interest` rebuilt from the stored record
  with the same random sort key, place counts and digest kept per key. It then replays the
  tracked keys in up to 8 rounds and settles once a round takes at most 1024 keys. It aborts and
  deletes the candidate on `Overflow`, on a store error, or when free space would drop below
  `min_free_mb` (checked before the copy against the index size and after every batch); after 8
  unsettled rounds it returns `Postpone` and keeps the candidate and tracking for a later
  `replay`.
- Search index generations: the first is `profiles-idx.db`, version N above 1 is
  `profiles-idx-vN.db` with a `generation` row (version, complete). The node holds
  `search.lock` beside them while it runs, so a second `dyappd` on the directory refuses to
  start. At start the highest complete generation is active and every incomplete or lower one is
  deleted; a complete generation newer than the binary's index version refuses the start and
  names the file. When the active version is below the binary's, a background worker
  (`rust/bootstrap/src/search/worker.rs`) builds the candidate and activates it under the writer
  lock: it replays the last tracked keys (at most 1024, more postpones again), switches the
  writer and the reader pool (a reader of the old generation is closed, not pooled), resets the
  write counters, recomputes the size limit against `search.max_bytes` and deletes the old
  generation. A larger schema thus holds fewer profiles in the same budget. Searches and writes
  go on meanwhile. A postponed candidate is replayed after 10 minutes, any other abort rebuilds
  after an hour. Three aborts of one kind in a row force the next attempt: three postpones or
  three overflows stop the world (build or take the candidate, then replay every tracked key
  under the writer lock, however many); three free-space aborts drop the index for an empty
  candidate and set `refilling`. A store error never forces. Each forced step logs a warning
  with `step` = `stop_the_world` or `drop_refill`. While `refilling` is set, every query reply is
  `partial` and the next catch-up tick starts a refill round; once a round in which a peer
  answered an inventory or an OK digest has no digest, inventory or catch-up request pending
  and no queued key left, the flag is
  cleared. The flag survives a restart. Incremental vacuum keeps its page budget and network
  repair keeps its traffic cap.
- Format versions (implemented, [Storage](#storage)): each file holds its format in
  `PRAGMA user_version`, one number that only grows; there is no downgrade, a fix ships as a
  higher version. On open the node runs the numbered steps from the file's version to its own, in
  order, one transaction each. A cheap step (a new column with a default, an index, a table) runs
  in place. A binary that finds a file newer than it knows refuses to start and names the file.
  Planned: a step that rewrites rows builds the new file next to the old one (for example
  `profiles-v2.db`) from the stored signed records, in the background under the I/O budget, while
  the old file answers and takes writes; the builder then copies the rows written since it
  started (by rowid or `seq`), switches under a short lock and deletes the old file. Deletes during
  the build (acks, expiry, eviction) go to both files, so nothing deleted comes back. The build
  starts only with the old file's size plus `min_free_mb` free, otherwise it waits and logs why.
  The search index already rebuilds this way on an index version bump (above).
- Activity: one UPSERT of a user's last-active date per day.

### Limits and abuse

Record sizes have separate client-send and network-accept bounds. Today profile payloads use
1 MiB for both and mailbox envelope payloads use 100 KiB for both. Search client publication
uses `search.max_profile_bytes`, at most the fixed 64 KiB network accept bound; both count the
encoded signed record. Media client uploads allow 6 MiB, while whole-copy replicas and signed
Keep/Attach proofs allow 1 MiB; larger uploads use shards. Wire framing has separate caps
([served protocol](#served-protocol)). `info` advertises client-send limits and `TOO_LARGE`
reports the bound checked for the refused object in `max_bytes`.

A size change does not require a new protocol ID, but needs an ordered node rollout:

- **Raise:** release N raises network acceptance and any necessary wire/decoder bounds while
  keeping send unchanged. Release N+1 raises send only after most holders run N. For search,
  raise `SEARCH_ACCEPT_BYTES` first, then the config default. Older holders can still refuse
  the larger replicas, leaving fewer copies until upgrades and repair fill the gap.
- **Lower:** release N lowers send while keeping acceptance for stored records, replication
  and repair. Release N+1 lowers accept no earlier than one applicable retention TTL after N
  (profiles default to 30 days; mailbox retention is separate). Verify that oversized retained
  records have expired or been republished; a live record refreshed through heartbeats may
  outlast that window; an old cached record can also be offered again by catch-up. Lowering both at once is an emergency operator decision and can reduce
  the copy count for larger records until they expire.
- **Shard layout:** the 1 MiB unit participates in K = ceil(length / 1 MiB), so it cannot be
  changed as an ordinary size limit. A future additive `shard_size` manifest field must default
  to 1 MiB for old records, with readers deployed before writers change the layout.

Validation of reported bounds exists in `dyapp_p2p_net::hints` but no client calls it yet;
client shrink/retry is planned
([evolution checklist](protobuf-schema.md#evolution-rules-proto3)). Capacities are space, not compatibility: a mailbox up to 10 MiB per device, signals up to `limits.signal_max_kb`,
media up to `limits.media_per_owner_mb` per user, each counted by the node over the data it
holds. Write quotas apply per sender key and per IP group; the prefix length (for example /24
or /48) is the operator's choice.
Resource guards cap disk per store and for media (with a free-space reserve), traffic (request rates and
an optional byte rate; near it the node sheds media first, then search, the mailbox last) and
memory (connections, streams, request size). A full store answers "full" so the client tries
another replica.
Signals: a like, a view or any other signal is an ordinary end-to-end encrypted
mailbox envelope with `class = signal`, at most 1 KiB, so a node cannot tell a like from a view.
Signals have their own quota per mailbox, `limits.signal_max_kb` (default 2048, about 8 000
signals), apart from the 10 MB for messages: at the quota the node deletes the mailbox's
signals instead of answering "full", so the likes of a popular profile never block its messages:
tokenless ones first, from the sender label holding the most bytes, tokened ones only when none
remain ([storage](#storage)); the labels are the envelope's `via` and source group
(see the tokenless pool below).
In a mixed network a node older than signals ignores `class` and stores a signal as a message;
a client tells such a node by `max_signal_bytes = 0` in `info`.
The client sends at most one view per profile a day. Implemented: the disk, traffic and
connection guards in [Resource guards](#resource-guards) and the quotas and peer bans in
[Rate Limiting](#rate-limiting).

The operator may refuse service to any user through a deny list (implemented, see
[Deny list](#deny-list)) on their own node only. Nodes can exchange signed lists, with separate
switches to share and to accept, both off by default; a node only stores received lists and acts
on none of them. The exchange is not part of the first release.
Listing a media blob hash removes its local whole copy, manifest and erasure-coded shards on
reload; the node refuses new puts and reads of that hash.

### Operating a node

- Runs as an unprivileged system user and refuses to start as root; default port 33445, every
  path must be writable by that user (implemented in `dyappd`). The Debian 12 package adds a
  hardened systemd unit and the `dyappd` system user
  ([deployment](../manuals/bootstrap.md#debian-12-package)).
- A TOML config sets addresses, paths per store, roles, limits and TTL; environment variables
  override it and unknown keys are ignored, so a rolled-back node still starts. Invalid config
  fails at startup ([Configuration](#configuration)).
- The node key is a libp2p key made once by `dyappd keygen` with the node-ID proof of work: a
  file (mode 0600) at `storage.key`, by default in the data directory, or, from the package, a
  `systemd-creds` credential the unit decrypts (`keygen -` pipes it to `systemd-creds encrypt`,
  [ADR 0018](../decisions/0018-node-key-source.md)). The node does not start without it or with
  a key file that has group/other permission bits. It checks the opened file and suggests `chmod 600` without
  changing permissions. Owned serialization/loading buffers are cleared on drop.
  A new key is a new node: the node refuses a key that does not match the stored data, and the
  operator deletes the data. On a leak or a move the operator creates a new key.
- <a id="storage-network"></a>The data directory belongs to one network: the `network` file in
  it holds the name (empty for the public network, no newline). `keygen` or the first start
  writes it and never replaces it; a start under another `network.name`, or a malformed or
  oversized file, is refused with "use a new storage.dir". A directory without the file but with
  data (a store, its `-wal`/`-shm`/`-journal` sidecars, `data/` or the old `media/` layout) predates
  network names and belongs to the public network; the deny list, peer caches and the key are not
  data.
- Settings live in the config file and the systemd unit and need a restart. The `dyappd` binary
  also carries the admin subcommands: `deny add|remove|list` edits `deny.db`, which the running
  node applies within 10 seconds and logs ([Deny list](#deny-list)), `deny received` prints the
  lists other nodes shared, and `status` prints what
  the stores hold, read-only. There is no TUI and no admin HTTP endpoint; metrics go to the log.
- <a id="default-port"></a>Default port 33445 for the first release, chosen by the maintainer.
  It binds without privilege and serves TCP and QUIC on one number. Explicit addresses on the
  former port 7070 still work; operators must update their firewall and seed entries when
  changing a node's listen address. Changing the port alone
  does not resist DPI: a filter matches the libp2p handshake and QUIC Initial on any port
  ([RFC 9312](https://www.rfc-editor.org/rfc/rfc9312.html),
  [RFC 9505](https://www.rfc-editor.org/rfc/rfc9505.html)). Port 443 may pass port-based
  filters but needs `CAP_NET_BIND_SERVICE` or a redirect, competes with a web server on the same
  host and still shows a non-TLS/non-HTTP/3 handshake. A network that drops non-standard ports
  is served by the operator's `listen` choice (any port works, seeds carry it). Transport
  camouflage is research, with the client proxy in the
  [threat model](../security/threat-model.md); it needs no new default.

## Data Model

### Mailbox envelope

A message for one device is a `SignedRecord` signed by a sender key over
`"dyapp/envelope/v1\0" || payload`, where the payload is an `Envelope`
([schema](protobuf-schema.md#node-protocol)):

```
Envelope { id: 16 random bytes, mailbox: SHA-256(recipient device key), ciphertext,
           class, write_token: optional SignedRecord }
WriteToken { mailbox, token_key, epoch: UTC day number }  // signed by the recipient device key
Revoke { mailbox, token_keys: 1 to 64, created_at }       // signed by the recipient device key
Revoke { mailbox, created_at, owner }                     // closes it; signed by the identity key
owner: identity key                                       // signed by the device key at linking
```

**Write tokens.** A recipient hands each contact a daily token, signed by its device key over
`"dyapp/write-token/v1\0" || payload`. A node checks a present token on every put: the token
signature, `SHA-256(token signer) == token.mailbox == envelope.mailbox`, and that the envelope is
signed by `token_key`. A client put also needs the current time within the token's day ± 1 h;
replica paths (`replica_put`, repair, a local store) accept a token until the day's end +
`limits.message_ttl_hours` + 1 h. On a client put a forged token answers `DENIED` with a strike,
an expired one `STALE` without a strike; on replica paths any token error drops the envelope
silently, without a strike and without changing the batch status. The token counts towards the
same 100 KiB signed-payload bound; the client reserves 256 bytes for it.

**Revocation.** A recipient that blocks a contact sends `revoke`, signed by its device key over
`"dyapp/mailbox-revoke/v1\0" || payload`, naming up to 64 token keys. It needs no nonce: a
replay only revokes the same keys again. The node deletes the stored envelopes of those keys and
refuses their tokens until `created_at` + 31 days + `limits.message_ttl_hours` + 1 h, which covers
tokens issued up to 30 days ahead and their replica circulation; a key named again keeps the
later expiry. A client put with a revoked token gets `DENIED` without a strike; replica paths
drop it silently. The node forwards a valid `revoke` verbatim as `replica_revoke` like an ack,
and repair sends a mailbox's unexpired revokes, one request unit each, before its envelopes. A
mailbox holds at most 4096 revoked keys and 4096 signed revokes (`FULL` beyond); expired ones
free room. The node sees which token pseudonyms a mailbox blocked, not who they are.

**Closing a revoked device's mailbox.** When the owner drops a lost or sold device from the
profile, another of its devices sends a `revoke` with no token keys, signed by the identity key,
whose `owner` is the binding the device signed when it was linked: its identity key as the
payload, signed by the device key over `"dyapp/device-owner/v1\0" || payload`. The binding's
signer must hash to `mailbox` and its payload must be the revoke's signer, so only the identity
the device accepted can close it, without the lost device. A device certificate alone would not
do: anyone can certify a public device key under their own identity. The node deletes every
stored envelope of the mailbox and, until the same expiry as a revoke, answers its `fetch`, every
`put` and repair `inventory` with `REFUSED`; replicas and repair carry the close like any revoke.
A replay closes the same mailbox again. A stale sender profile can still address the closed
mailbox; its puts are refused, and by the expiry its senders have the newer profile. The close is
stored as a revoked empty token key, so it shares the 4096 cap.

**Tokenless pool.** Each stored envelope carries two 8-byte eviction labels: `via =
SHA-256(peer id)[..8]` of the peer that handed it in (the node's own id for a local store), and
`source_group = SHA-256(salt || mailbox || IP group)[..8]`, set by the node that accepts the
client put. The salt is random, kept in memory and new on every start, so a label cannot be
linked across mailboxes or restarts and needs no storage; repair after a restart mixes old and
new labels for a while, which only coarsens eviction. Replicas pass the label unsigned in
`Envelopes.source_groups` (parallel to `envelopes`; missing reads as empty, longer than 8 bytes
is stored empty), and repair sends the stored one. A peer can spray labels, but they all sit
under its own `via`, so it evicts only its own envelopes. With `limits.require_write_tokens`
(default false) tokenless messages count against a pool of `limits.mailbox_pool_kb` per mailbox
(default 1024, at least 101) evicted by the pool rule ([storage](#storage)), and tokened ones
alone against the 10 MiB, so a flood of first messages cannot fill the mailbox. After each
stored tokenless envelope the node also trims tokenless envelopes store-wide to
`limits.tokenless_share_pct` of `limits.messages_max_mb` (default 20 %): signals only while the
flag is off, messages too while it is on. In a network with mixed flags a node with the flag off
stores tokenless messages up to the 10 MiB as before; a sender cannot tell, and `info` reports
the flag and the pool.

The node stores the signed record as received, once per (mailbox, id), and never learns the
sender's identity: the sender key only proves someone signed it; puts are limited per sender key
([Rate Limiting](#rate-limiting)). `rust/client` pads the plaintext to the next power of two from
1 KiB before encryption (`chat::pad`) and signs every envelope with a fresh key it does not keep
(sealed sender), never its published device key
([Threat Model](../security/threat-model.md#traffic-analysis)).

**Lifecycle:** put → stored until the device acks it or `limits.message_ttl_hours` passes
(`dyappd` deletes expired envelopes every hour) → fetched messages first, then signals,
oldest first in each. Repair carries the original expiry: `Envelopes.remaining_seconds`, parallel
to `envelopes`, gives each envelope's seconds left, and the receiver stores it until `now +` that
time, capped at its own TTL; 0 drops the envelope. A list of another length, or none from an
older node and on the acceptor's fan-out, gives a fresh TTL, so in a mixed network an envelope can
outlive the TTL by at most one more TTL. A re-sent envelope the node already holds keeps its
expiry.

`Envelope.class`, `ENVELOPE_CLASS_MESSAGE` (default, also any unknown value) or
`ENVELOPE_CLASS_SIGNAL`, is the only field that tells a node a signal from a message. A signal's
signed payload is at most 1 KiB (else `TOO_LARGE` with `max_bytes` 1024) and counts against
`limits.signal_max_kb`, not the mailbox's 10 MiB ([Limits and abuse](#limits-and-abuse)).

The client signal sender is planned. Its size guard in `rust/client/src/chat.rs` encrypts real
MLS application messages and measures the encoded envelope including a signed write token. With
256-byte plaintext padding, a 16-byte like payload is 638 bytes. With 512-byte padding, an
introduction carrying up to 508 bytes of total plaintext is 894 bytes. The four-byte length prefix
is included in each padding bucket. The existing chat padding floor of 1 KiB makes even the small
like envelope 1406 bytes, so the signal sender must use its own shorter padding and reject payloads
that do not fit; it must never truncate ciphertext or raise the node's limit. These measurements
use the current OpenMLS and protobuf versions and are guarded by a test rather than assumed stable.

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
country, place (city or district, no coordinates), income range, kids, goals, interests,
profession, company, industry, career stage and work-life balance); every field is
optional and public by design
([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).

`/dyapp/profile` `publish` stores a record only if:

- the signature verifies against `public_key`;
- the payload is at most 1 MiB and decodes as a `Profile` (media are separate blobs on `/dyapp/media`);
- `version` ≥ 1 and greater than the stored version for that peer ID (otherwise `STALE`);
- content is sane: country is an ISO 3166-1 alpha-2 code, income range not reversed,
  `education_level` at most 6, place at most 1024 characters, other strings at most 64 and at most 32 `interests`, every
  string passing the text rule above, at most 16 `photos`, each a
  32-byte thumbnail hash and 1–64 full-image blob hashes, a tombstone (`deleted = true`)
  carries no other field but the proof of work (`pow_nonce`, `pow_bits`).

Deletion publishes a tombstone with a higher version. The server keeps the tombstone
so an older version cannot be re-imported; `get` returns it so peers learn of the deletion.
A profile, tombstone included, is deleted `limits.profile_ttl_days` (default 30) after its
owner was last seen: a publish of a newer version or an identity-signed heartbeat whose time
is within 10 minutes of the node clock and newer than the stored `last_seen`. Heartbeats advance
`last_seen` to their signed time, so repeating one does not extend retention. Stale republishes,
media requests and mailbox requests do not refresh profile liveness. The client is
meant to send a heartbeat to the profile's replicas at most once a day, at a random time rather
than on app open, so `last_seen` does not track app use (planned; no client yet).
Store admission checks every positive proof on client publish and node-to-node replica put.
Confirmed (key, nonce) proofs are cached across profile versions; a changed nonce is checked
again, while a failed proof is never cached. Local holders store only admitted records. Busy
proof workers answer `RATE_LIMITED` without striking the sender. Tombstones follow the same
proof rule; heartbeats change liveness only as described above.
Full field table: [Privacy & Metadata Visibility](../security/privacy.md).

## Served protocol

Source: `rust/bootstrap/src/service.rs` and `service/` (request handling), `node.rs` and
`node/` (the libp2p loop), `rust/p2p-net` (`ProtoCodec`, protocol IDs), `storage.rs` (SQLite), `media.rs` (blobs).
`dyappd` serves these libp2p request-response protocols over TCP and QUIC, one protobuf request and one reply
per stream ([schema](protobuf-schema.md#node-protocol)). The IDs below are the public network's;
a node with a `network.name` serves each as `/dyapp/net-<ns>/<p>`
([ADR 0017](../decisions/0017-network-names.md)). Each request and reply is capped at
1 MiB + 64 KiB, except `/dyapp/media`, which allows 6 MiB + 64 KiB. These are wire limits;
the payload and storage limits below are checked separately.

The reply schemas include additive `server_time` Unix-seconds fields in `NodeInfo` and
profile/media/search responses ([clock hints](protobuf-schema.md#clock-hints)). `info` reports
the node clock; signed-time `INVALID` replies report the clock snapshot used in validation.
Other failures leave the field zero. Client clock correction from three distinct nodes remains
planned.

`NodeInfo.capabilities` names optional features beyond the roles: `PROFILE_HEARTBEAT` with the
store role, `SEARCH_PUZZLE` with the search role, `SHARED_DENY_LIST` with
`network.share_deny_list`. Protocol evolution follows
[ADR 0016](../decisions/0016-protocol-evolution.md): a change that tightens security ships as a
new `oneof` variant that older nodes answer `UNSUPPORTED`; every `NodeInfo` value is a hint from
an untrusted node that may raise a client's PoW (up to 20 bits) or lower its sizes, never the
reverse, and a hint out of range skips the node (`dyapp_p2p_net::hints`, not yet called by a
client); `server_time` is never used for a security decision; replica count and shard layout are
frozen per record and never taken from `NodeInfo`.

Mailbox `TOO_LARGE` replies include `max_bytes` for the checked signed-envelope payload,
including repair batches. Client sends use the advertised 100 KiB limit; replica/local admission
uses a separate accept constant, also 100 KiB today. A batch containing a forged envelope still
answers `DENIED` with no size hint. Profile publish and replica/local stores also return the
checked payload bound in `TOO_LARGE`: their send/accept constants are separate, both 1 MiB
today, and node info advertises the send limit. Search publication reports its checked encoded-record
bound in `max_bytes`; insufficient profile PoW reports `required_pow_bits`, while other invalid
content does not. Media service rejections report 6 MiB for client-upload data and 1 MiB for
replica data or encoded Keep/Attach authorizations. Oversized shards retain `INVALID` manifest
validation. The node's upload dispatch preserves the 6 MiB hint from admission.
The current protocol replies are:

| Protocol | Request | Reply |
|----------|---------|-------|
| `/dyapp/node` | `info` | `STATUS_OK`, roles (`ROLE_STORE`, `ROLE_MEDIA`, `ROLE_SEARCH`), `max_media_bytes` (6 MiB, media role only), `max_search_profile_bytes` (`search.max_profile_bytes`, search role only), `min_profile_pow` (`search.min_profile_pow_bits`, enforced by store and search), `min_heartbeat_pow` (`search.min_heartbeat_pow_bits`, search role only), `puzzle_bits` and `puzzle_challenge` (search role only, 0 while off), `max_profile_bytes` (1 MiB), `max_message_bytes` (100 KiB), `max_mailbox_bytes` (10 MiB), `max_signal_bytes` (1 KiB), `signal_quota_bytes` (`limits.signal_max_kb`), `mailbox_pool_bytes` (`limits.mailbox_pool_kb` in bytes), `require_write_tokens` (`limits.require_write_tokens`), `retention_seconds` (`limits.message_ttl_hours`); a node without the store role serves no protocol |
| `/dyapp/node` | `turn` | `OK` with coturn credentials and URLs ([TURN](#turn-credentials)); `RATE_LIMITED`; `REFUSED` a denied peer; `UNSUPPORTED` without the turn role and on older nodes |
| `/dyapp/node` | `deny_list` | `OK` with the signed list when `network.share_deny_list` is on, else `NOT_FOUND` ([exchange](#deny-list-exchange)) |
| `/dyapp/profile` | `publish(SignedRecord)` | `OK` after two distinct stores (one only with no routing peers); `FULL` pending cap or 10-second timeout; `STALE` with a verified equal/newer holder record; `DENIED` bad signature or forged PoW (strike); `INVALID` bad key or content, or weak PoW with `required_pow_bits`; `RATE_LIMITED` when proof workers are busy; `TOO_LARGE` with checked `max_bytes` for payload over 1 MiB |
| `/dyapp/profile` | `replica_put(SignedRecord)` (node to node) | stores and verifies like publish, never forwards; costs one request unit; same validation replies |
| `/dyapp/profile` | `get(peer_id)`, 32 raw bytes | `OK` with the record, tombstone included; `NOT_FOUND`; `INVALID` wrong length |
| `/dyapp/profile` | `heartbeat(SignedRecord)`, payload `Heartbeat { time }` signed by the identity key | `OK` advances `last_seen` to the signed time if newer; `NOT_FOUND` no profile, publish it; `DENIED` bad signature; `INVALID` time more than 10 min off; `REFUSED` denied key; older nodes `UNSUPPORTED` |
| `/dyapp/mailbox` | `challenge` | `OK` with a fresh 32-byte nonce for this connection; it replaces the previous one |
| `/dyapp/mailbox` | `put(SignedRecord)`, payload `Envelope` | `OK` after two distinct stores, also for a repeated (mailbox, id); one local store only with no peers; `FULL` pending cap or 10-second timeout; `DENIED` bad signature or forged write token (strike), revoked write token (no strike); `STALE` write token outside its day ± 1 h; `INVALID` undecodable, id not 16 or mailbox not 32 bytes; `TOO_LARGE` with checked `max_bytes` for payload over 100 KiB; `FULL` the mailbox would exceed 10 MiB |
| `/dyapp/mailbox` | `fetch(SignedRecord)`, payload `Fetch` | `OK` with the oldest envelopes (at most `limit`, 100 and 1 MiB per reply) and `more`; `DENIED` |
| `/dyapp/mailbox` | `ack(SignedRecord)`, payload `Ack` | `OK`, the listed ids are deleted, unknown ones ignored, and the ack is forwarded as `replica_ack`; `DENIED`; `INVALID` an id not 16 bytes |
| `/dyapp/mailbox` | `replica_ack(SignedRecord)` | an `ack` another node forwards verbatim: checked like `ack` without the nonce, not forwarded again; same replies |
| `/dyapp/mailbox` | `revoke(SignedRecord)`, payload `Revoke` | `OK`, the keys are revoked and their envelopes deleted (with `owner`, the mailbox is closed: all envelopes deleted, fetch, put and inventory `REFUSED`), and the revoke is forwarded as `replica_revoke`; `DENIED` bad signature, a signer that does not own `mailbox` (or with `owner`: a binding not signed by the mailbox's device key or naming another identity) or over 64 keys (strike), `created_at` over 1 h ahead (no strike); `INVALID` no keys without `owner`, keys with it, or a key not 32 bytes; `STALE` already expired; `REFUSED` a denied mailbox; `FULL` over 4096 keys or revokes ([Revocation](#mailbox-envelope)) |
| `/dyapp/mailbox` | `replica_revoke(SignedRecord)` | a `revoke` another node forwards verbatim or repair carries: checked like `revoke`, not forwarded again; same replies |
| `/dyapp/mailbox` | `inventory(Inventory)`: mailbox, at most 10 000 ids (node to node) | `OK` with `missing`, the listed ids the node does not hold, read from stored ids without the envelopes; `DENIED` and a strike from a peer without node proof of work; `INVALID` mailbox not 32 bytes, an id not 16 or too many ids; `REFUSED` a denied mailbox; `RATE_LIMITED` from 75 % of the traffic cap |
| `/dyapp/mailbox` | `replica_put(Envelopes)` (node to node) | each envelope stored and verified like a `put`, never forwarded, with its unsigned source group from `source_groups`; an envelope with a forged, expired or revoked write token is dropped without a strike; one peer request unit per batch; `OK`, else `DENIED` when any signature is forged, else the first failure; `TOO_LARGE` includes checked `max_bytes` |
| `/dyapp/mailbox-push` | `MailboxPush` (node to client) | the envelope just stored, sent once per connection that sent a `fetch` with `watch` |
| `/dyapp/media` | `keep(SignedRecord)`, payload `MediaKeep` | `OK` after bounded repair, with `missing` hashes that lack a recoverable copy/K shards or could not be checked; `STALE` older version or repeated time; the same version and list with a newer signed time refreshes liveness; `DENIED` bad signature; `INVALID` bad version, over 256 hashes, bad hash length or time more than 10 min off; `TOO_LARGE` with `max_bytes` for encoded signed proof over 1 MiB |
| `/dyapp/media` | `put(MediaPut)`: owner = SHA-256 of the identity key, data | `OK` after two distinct whole-copy stores, or two manifests and K shards (one manifest only when isolated); `NOT_FOUND` no signed authorization for SHA-256(data); `TOO_LARGE` with `max_bytes` over 6 MiB; `FULL` pending cap, deadline, billed owner quota or disk guard; `INVALID` owner not 32 bytes |
| `/dyapp/media` | `replica_put(MediaReplicaPut)`: whole data or manifest, and signed keep or attachment | independently verified and stored without forwarding; `DENIED` missing/invalid signature; `NOT_FOUND` hash absent from proof or latest keep; `INVALID` expired proof, malformed or conflicting manifest; `TOO_LARGE` with `max_bytes` for whole data or signed proof over 1 MiB; other put storage statuses apply |
| `/dyapp/media` | `shard_put(MediaShardPut)`: signed manifest replica, index, data | `OK` after proof, manifest hash and shard length/hash validation; never forwarded; invalid data gives `INVALID` |
| `/dyapp/media` | `have(MediaHave)`: at most 256 hashes/2816 parts | `OK` with an inventory of allowed present/missing parts, manifests and whole-copy availability; untrusted/far entries omitted; `INVALID` malformed or oversized batch; `RATE_LIMITED` traffic budget |
| `/dyapp/media` | `shard_get(MediaShardGet)`: hash, index | `OK` with at most 1 MiB; `NOT_FOUND`; `RATE_LIMITED` reply byte budget; `INVALID` malformed hash or index |
| `/dyapp/media` | `get(GetMedia)`: hash, optional `offset` and `len` | `OK` with the blob, or with `len` the piece of at most `len` bytes from `offset`, and `total_size` ([ranged get](#ranged-media-get)); `RATE_LIMITED` reply/assembly budget or four active assemblies; `NOT_FOUND` missing blob, insufficient shards or invalid decoded hash; `INVALID` hash not 32 bytes, `len` over 1 MiB, `offset` without `len` or at or past the end |
| `/dyapp/media` | `attach(SignedRecord)`, payload `MediaAttach` | `OK` with `missing`; `DENIED` bad signature; `FULL` 256 unreleased attachments of the sender; `INVALID` release hash or a blob hash not 32 bytes, no or over 16 hashes, `created` over 10 minutes ahead; `TOO_LARGE` with `max_bytes` for encoded signed proof over 1 MiB |
| `/dyapp/media` | `release(MediaRelease)`: secret | `OK` attachments dropped; `NOT_FOUND` none with SHA-256(secret) |
| `/dyapp/search` | `publish(SignedRecord)` | `OK` stored, or skipped for a topic the node does not keep, then gossiped; `STALE` not newer than the held version; `RATE_LIMITED` with `retry_after` within 10 minutes of the last update, or 1 when both proof-of-work slots are busy; `DENIED` bad signature or forged proof of work, which strikes the peer; `INVALID` bad content, or with `required_pow_bits` when `pow_bits` is below `search.min_profile_pow_bits`; `TOO_LARGE` with `max_bytes` over `search.max_profile_bytes`; `REFUSED` denied key; `UNSUPPORTED` without the search role |
| `/dyapp/search` | `heartbeat(SignedRecord)`, payload `Heartbeat { time, version, pow_nonce, pow_bits, pow_day }` | `OK` refreshes the held record and is gossiped; `NOT_FOUND` no record or an older version, publish it; `RATE_LIMITED` with `retry_after` over 2 heartbeats a day; `DENIED`; `INVALID` with `server_time` for time more than 10 min off, or with `required_pow_bits` when the stored profile's difficulty is below the current threshold (no refresh or removal), or for a heartbeat proof that is missing, below `min_heartbeat_pow` or not for the current UTC day; `DENIED` also for a forged heartbeat proof; `REFUSED`; `UNSUPPORTED` |
| `/dyapp/search` | `query(SearchQuery)` | `OK` with hits (key hash, version), the applied fields and the index version, `partial` when the `search.max_scan_rows` budget ran out or the index refills; run off the swarm loop; `RATE_LIMITED` with `retry_after` 1 when the search queue refuses it; `PUZZLE` with `puzzle_challenge` and `puzzle_bits` while the node is loaded and `puzzle` is missing, wrong or used ([Search](#search)); `UNSUPPORTED` |
| `/dyapp/search` | `get(SearchGet)`: at most 50 key hashes | `OK` with the held records cut at 1 MiB, denied keys omitted, off the swarm loop; `INVALID` too many keys or a key not 32 bytes; `RATE_LIMITED` as for `query` or over the reply budget; `PUZZLE` as for `query`; `UNSUPPORTED` |
| `/dyapp/inventory` | `InventoryRequest { topic, after, before }` (node to node) | `OK` with up to 10 000 (key hash, version) pairs of the topic after `after` and, unless empty, before `before`, sorted by key; `INVALID` topic out of range or `after` or `before` over 32 bytes; `RATE_LIMITED`; `REFUSED`; `UNSUPPORTED` without the search role |
| `/dyapp/digest` | `DigestRequest { topic, bucket }` (node to node) | `OK` with 256 hashes of 32 B: the topic's buckets, or with `bucket` its sub-buckets; `INVALID` topic out of range or `bucket` over 255; `RATE_LIMITED`; `REFUSED`; `UNSUPPORTED` without the search role |

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
  `inventory` of the ids it holds; the mailbox's unexpired revokes follow as `replica_revoke`
  and the envelopes the answer lists as `missing` in one `replica_put`, again only to a trusted
  node. The peer, if one of the first five nodes its own
  routing table holds for a replica key is the requester and it trusts the requester, sends back
  the envelopes the inventory lacks the same way. A batch is cut at 1 MiB, the rest waits for the
  next visit; repair bytes count towards the traffic cap. A repaired envelope keeps its original
  expiry ([lifecycle](#mailbox-envelope)). Old nodes answer both requests `UNSUPPORTED`, and repair skips them. The
  inventory lists messages and signals, but repair sends messages only: a signal evicted at its
  quota would come back from another replica and evict the next one. In a mixed network an older
  peer still sends signals back, bounded by the 1 MiB per visit and the TTL, until it upgrades.
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
  strike) besides the shared ones. A node without the media role answers `UNSUPPORTED`. A get
  can ask for a piece ([ranged get](#ranged-media-get)).
- **Chat attachments.** A signed `attach` lists up to 16 blobs under the SHA-256 of a release
  secret; the blobs are put like listed ones and count against the sender's quota. Anyone holding
  the secret sends `release` to each holder and the node drops the
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
  indexed by owner on read). Listing a blob hash purges its local whole copy, manifest and shards
  on reload, while keeping keep and attachment rows as missing; `get`, `shard_get`, `put`,
  `shard_put` and replica puts of that hash return `REFUSED`. Repair skips listed hashes, and
  `dyappd status` reports the cumulative `purged_blobs` count. Removing the entry does not
  restore the data; a permitted owner may upload it again.
- <a id="deny-list-exchange"></a>**Deny-list exchange** (not part of the first release; both
  switches stay off). With `network.share_deny_list` on, a
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
  `use-auth-secret` scheme: username `<expires>:<group>`, password base64(HMAC-SHA1(`turn.secret`,
  username)), valid `turn.credential_minutes` (default 60), with the `turn:`/`turns:` URLs from
  `turn.urls`; the [manual](../manuals/bootstrap.md#turn-relay) has a coturn example with its quotas.
  `<group>` is the first 8 bytes, in hex, of HMAC-SHA1(`turn.secret`, `ip-group:` and the asker's IP
  group): fresh peer IDs from one group share coturn's `user-quota`, and the username shows no
  address. Rotating `turn.secret` renews every pseudonym.
  The request is rate-limited like a profile get. Once its routing table has a peer the
  node announces itself as a Kademlia provider of `/dyapp/turn`; kad republishes the record every
  12 hours. Clients find relays with `get_providers` on that key (`dyapp_client::Node::providers`)
  and ask one for credentials with `dyapp_client::Node::turn`; using them for call ICE is planned.
- **Profile and mailbox requests are rate-limited** per remote libp2p peer ID and IP group:
  client puts cost five units and other requests one, with mailbox puts also limited per sender
  key (`RATE_LIMITED`); a banned peer is disconnected ([Rate Limiting](#rate-limiting)).
  A node without the store role answers `UNSUPPORTED` on both. `/dyapp/node` `info` counts only
  against the IP-group bucket and `deny_list` against both; a denied peer gets `REFUSED` for both.
- **Storage errors drop the request:** the client sees the stream close and tries another node.
- **Store requests run off the swarm loop.** `/dyapp/profile`, `/dyapp/media`, `/dyapp/mailbox`
  and `/dyapp/inventory` requests queue per remote peer and run on 4
  blocking workers (`node/jobs.rs`): one request per peer at a time, round robin between peers,
  so a peer flooding requests holds at most one worker and its replies keep its request order.
  A peer with 16 requests waiting gets the next one dropped (the stream closes; the client
  retries). A put at the in-flight replication cap gets `FULL` at once. Search `query` and
  `get` run on their own fair queue and two workers. Replication replies, gossip,
  repair, media inventory and maintenance still call SQLite on the loop. `/dyapp/node` touches no
  store and is answered on the loop, so it stays prompt while every worker is busy.
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
| envelope `write_token` | recipient device key | signature, mailbox, envelope signer and day; optional |
| envelope `ciphertext` | sender | size only; the recipient checks the sender inside it |

- **Profiles are owner-only.** Only the holder of the identity key can publish, replace or delete
  (tombstone) a profile. A replayed old version is rejected.
- **Age.** Age is self-declared; the client refuses users under 18 in its UI, and 30+ is only the
  target audience ([ADR 0012](../decisions/0012-private-p2p-interactions.md)). The node checks
  nothing beyond the signature: it would accept `17`.

### Client commands

`dyappd` is also a libp2p client for operators and scripts
(`rust/bootstrap/src/bin/dyappd/client.rs`). These commands read the config only for
`network.name`: they need neither the node key nor the owner of `storage.dir`. Each prints one
JSON object:

```
dyappd sign-profile [secret hex version] → {"peer_id", "record", "secret"} (test identity, hex protobuf)
dyappd heartbeat <addr> <secret hex> → {"status"}
dyappd info    [::1]:33445                → {"status", "peer_id", "server_time", "roles", "max_profile_bytes"}
dyappd publish <addr> <record hex>  → {"status"} (+ "record" when stale)
dyappd get     <addr> <peer_id hex> → {"status", "record"}
dyappd device-key                        → {"secret", "mailbox"}
dyappd put     <addr> <mailbox hex> <ciphertext hex> → {"status", "id"}
dyappd put-replicas <addr> <mailbox hex> → {"status", "id", "holders"}
dyappd put-local <addr> <mailbox hex> <ciphertext hex> [node-pow-bits] → {"status", "id"} (replica_put fixture)
dyappd watch   <addr> <secret hex>  → {"status", "id", "pushed"}
dyappd fetch   <addr> <secret hex>  → {"status", "ids", "more"}
dyappd ack     <addr> <secret hex> <id hex>... → {"status"}
dyappd closest <addr> <key hex>     → {"peers"}   (DHT lookup through the node)
dyappd replicate <addr> <peer_id hex> <record hex> → {"holders"}
dyappd flood   <addr> <n>           → {"<status>": count, "failed": count}
dyappd media   <addr> <data hex>    → {"keep", "put", "get"}
dyappd media-owned <addr> <data hex> → media statuses + details {owner, hash, keep_record}
dyappd media-sized <addr> <size> <32-byte seed hex> → same details; generated blob up to 6 MiB
dyappd media-keep <addr> <record hex> → {"status", "data", "missing"}
dyappd media-refresh <addr> <record hex> <secret hex> → fresh signed keep, statuses + missing + keep_record
dyappd media-get <addr> <hash hex>  → {"status", "data"}   (1 MiB pieces; CHANGED on a wrong hash)
dyappd turn    <addr>               → {"status", "username", "password", "urls", "expires"}
dyappd relays  <addr>               → {"providers"}   (TURN relays in the DHT)
```

`put` signs with a fresh sender key; `fetch` and `ack` get a challenge and use it on one
connection. `closest` lists the nodes that answered, closest first. `replicate` and `put-replicas`
look up the five holder keys, then send one client write to key 0's holder; only a failed write
tries the next holder. Their `holders` lists describe the lookup placement, while the acceptor
performs replication. `put-local` sends a non-forwarded mailbox replica for repair fixtures.
It generates a transport key with node-ID PoW before the network timeout begins: 22 bits by
default, or the optional `node-pow-bits` (0..32) for a test network. The network fixture passes 8.
`watch` sends a watching `fetch`,
puts an envelope to its own mailbox on another connection and prints the ids pushed back. `flood`
sends `n` profile gets at once on one connection. `media` has a fresh owner keep the blob's hash,
put the blob and get it back on one connection and prints each status; `"get"` is `"CHANGED"` if
the blob came back different.

`<addr>` is `host:port`, the host an IP (IPv6 in brackets, e.g. `[::1]:33445`) or a DNS name;
the client dials TCP. A multiaddr starting with `/` (e.g. `/ip6/::1/tcp/33445`) is still accepted.
A host without a port or an unbracketed IPv6 address is refused with the expected forms.

## Erasure coding (Reed-Solomon)

Profiles, mailboxes and media at or below `media.shard_threshold` (default 1 MiB) are replicated whole.
Larger media up to 6 MiB is encoded with `rust/bootstrap/src/replication.rs`: K = ceil(length /
1 MiB), M = 4. Each shard is at most 1 MiB and goes to the holder of
`replica_key(blob_hash, j)`; multiple indices may share a node. A manifest records the blob hash,
length and ordered shard hashes, derives K/M from the length, and is replicated to five blob
points. The first valid manifest wins; a different one answers `INVALID`. A verified whole copy
wins over manifests, including a whole put arriving after a manifest.

The acceptor confirms two distinct manifest holders and K distinct shard indices before `OK`;
only an isolated node may confirm one manifest. Remaining shards continue in the background
within the shared ten-second write deadline. Replica/shard puts carry the signed keep or
attachment and verify it independently. Whole data, signed authorization and every shard are
capped at 1 MiB. The receiver checks every shard's index, length and SHA-256 against its manifest.
Uploads and owner quotas cost ceil(bytes × (K+M)/K); whole copies continue to cost bytes × 5.

A get serves a verified whole copy first, otherwise looks up shard holders and assembles any K
valid indices. It checks the resulting blob SHA-256 before replying. A failed hash check removes
only the local manifest/shards, returns `NOT_FOUND`, and leaves ownership lists for reupload.
Missing shards or a ten-second assembly deadline return `NOT_FOUND` without discarding metadata.
At most four assemblies run at once. Assembly and reply bytes are reserved within the shared
75 % traffic budget before fetching (all K+M shard payloads are reserved conservatively).
Media RPCs are queued with at most two outstanding requests per peer and sixteen per node;
queued jobs keep shard data in the shared upload rather than cloning it until dispatch.
Keep-driven media repair is described above. `media_replica_holders` counts manifest holders
for the latest sharded write, not distinct shard holders or guaranteed fault tolerance.

<a id="ranged-media-get"></a>**Ranged get and assembly cache.** `get(hash, offset, len)`
answers one piece of at most 1 MiB with the blob's total size, so a client fetches a large blob
piece by piece and resumes after a break; without `len` it is the whole get. A blob assembled from
shards is kept as `<media dir>/cache/<hash>` for `limits.media_cache_minutes` (default 10) after
its last read, so later pieces read that file instead of assembling again. The cache counts towards
`media_max_mb` and is evicted first; the hourly cleanup deletes expired entries, and a deny-list
purge deletes the blob's entry. An invalid media file path returns a storage error instead of
panicking during the file write. Piece bytes count towards the traffic cap like a get; an assembly
reserves its shard bytes plus the piece. `dyappd media-get` fetches 1 MiB pieces, retries a
rate-limited piece from its offset and checks the hash once it has all pieces.

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

The media codec uses K = ⌈size / 1 MiB⌉ data shards and M = 4 parity directly, independently
of this general replication-factor mapping. Messages and profiles are replicated whole.

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

Media codec tests cover 1 MiB, 1 MiB + 1 byte and 6 MiB, four lost shards, invalid layouts,
shard hashes and final blob hashes. Nodes persist and serve shards through `/dyapp/media`.

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
  (defaults /24 and /48). The IP is the one that answered: for a dial by DNS name the resolved
  address, and an IPv4-mapped IPv6 address groups as IPv4. A request on a connection whose IP is
  unknown (no IP transport) is refused with `RATE_LIMITED` and counted under the
  `unknown_endpoint` guard, without a strike. Every unit spent from the IP-group bucket is also
  spent from a coarse-group bucket at ten times its rate, keyed by the same IP masked to
  `coarse_ipv4_prefix` / `coarse_ipv6_prefix` (defaults /16 and /32), so rotating /24s within a
  /16 or /48s within a /32 does not multiply the budget. `/dyapp/node` `deny_list` counts one
  unit against both; `info` counts only against the group buckets, with no strike, so discovery
  stays cheap. The deny list matches only fine groups.
- `/dyapp/search` and `/dyapp/inventory` count in units against the same two buckets: a query
  1, a publish or heartbeat 5, a get 1 per started 64 KiB of its reply, an inventory 10. Requests
  from a connected search node (one listing `/dyapp/search-kad` in identify) skip the units for
  get and inventory and are paid from the node's `maintenance.io_mb_per_s` byte budget instead.
- **Fair search queue.** Admitted `query` and `get` requests wait for the two index workers in
  a queue (`node/jobs.rs`): gets from connected search nodes first, then one request per IP
  group in turn. A group may have 4 waiting and the queue 256; past either the request gets
  `RATE_LIMITED` with `retry_after` 1 and no strike. Subscribers behind one carrier CGNAT (or
  one relay) share a group; `ipv4_prefix` tunes how wide it is.
- A mailbox `put` also counts against the envelope's sender key (`sender_puts_per_second`,
  default 10), after the signature is checked. With sealed sender a client signs each envelope
  with a fresh key, so this bucket binds only a sender that reuses one; the IP-group and mailbox
  quotas are the real bounds.
- libp2p keys and sender keys are free to generate; the IP-group bucket bounds what a client
  gains by rotating them. A relayed connection gets the relay's group. There is no Sybil
  resistance beyond that.
- Buckets idle for a second (refilled, so dropping them changes nothing) are removed once a
  second, and before a new one is added once a map holds 10 000 buckets.

**Replica eligibility.** Mailbox, profile and media `replica_put`, media `shard_put` and mailbox
`inventory` require a valid transport peer ID meeting this node's `network.id_pow_bits`; a
missing proof returns `DENIED` and a strike before record verification or a mailbox read.
Eligibility uses transport-ID proof of work; advertised roles do not change this check.
Replicas exceeding the peer, IP-group or media rate buckets return `RATE_LIMITED` without a strike;
bad signed records still return `DENIED` and strike. Sender caps cover regular mailbox/profile
fan-out, profile-gap repair, mailbox repair batches and media replica/shard writes, including repair.

**Local peer reputation.** Each non-replica refusal by the peer or IP-group bucket and each `DENIED` reply
(a bad signature, a replayed or missing mailbox nonce) is a strike against the peer ID. At
`strikes_to_ban` strikes (default 100), each within `ban_minutes` (default 10) of the previous one,
the peer is banned: the node closes its connections, closes any new ones, and answers
`RATE_LIMITED` to requests still in flight, until `ban_minutes` pass without a strike. Scores live
in memory, go with a restart and are never shared with other nodes
([ADR 0008](../decisions/0008-sybil-and-eclipse-defences.md)).

**Storage trust.** Replicas (forwarded acks, repair) go only to trusted peers. A peer is trusted
once the node first connected to it at least `network.storage_trust_minutes` ago (default 60; 0
trusts at once) and while its score is not negative. The score gains 1 per answer to the node's
mailbox requests and per signed `replica_put`, `replica_ack` or `replica_revoke` accepted from the peer, and loses 1
per mailbox request of the node that failed (an old node lacking the protocol does not count). A
peer is forgotten when its last connection closes while it is outside the routing table, and at
the next maintenance run once it has left both, so clients that rotate peer IDs do not pile up in
memory. Like the ban
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
  95 % of it: assembled blobs of the cache by oldest read, then unreleased chat attachments by
  oldest `created`, then the `keep` of owners seen longest ago, and answers `FULL` only if that is
  not enough; a blob still held by another `keep` or attachment stays. Eviction is bounded to 4096
  lists per category per put.
- **Traffic**: node protocol bytes in and out (encoded requests and replies, not transport
  overhead) are counted per second. Media fan-out prepays four additional payload copies at
  acceptance; forwarded payload is not charged again, and get payload is reserved before reply.
  With `bytes_per_second` set (default 0, no limit), media
  requests and repair get `RATE_LIMITED` from 75 % of it within the current second, profile
  requests from 90 % and mailbox requests at 100 %; `info` is always served. Search and
  inventory requests are shed with media, from 75 %, and catch-up is skipped while that share
  is busy. There is no monthly cap: a node is not told its billing
  period, and the byte rate bounds a month too.
- **Guard metrics**: a guard that starts or stops refusing logs once; each maintenance run logs
  per guard how many requests it refused since the last run (the byte rate, which flips every
  second under load, and `unknown_endpoint`, only here).
- **Incoming group connections**: `max_connections_per_ip_group` (default 32, positive)
  combines pending and established incoming connections within a fine IP group using
  `ipv4_prefix` / `ipv6_prefix`. Outgoing connections are exempt. Slots are freed on failed
  handshakes, composed guard refusals and close; endpoints without an IP share one bounded
  group. LAN routing settings do not disable the cap. The maintenance refusal log includes
  `group_refused` alongside the total; the state line exposes `incoming_group_connections`
  and `incoming_groups`. Real TCP tests verify the 33rd refusal and slot reuse; QUIC churn
  checks both counts return to zero after each close.
- **Coarse request budget**: a second request bucket at ten times the fine-group rate per
  `coarse_ipv4_prefix` / `coarse_ipv6_prefix` group (defaults /16 and /32, bounds 0..32 and
  0..128) ([Rate Limiting](#rate-limiting)). The fine-group rate is limited to 1..100000000, so
  the coarse bucket refills at most once a nanosecond, the precision of its clock.
- **Connections and memory**: libp2p connection limits — `max_connections` established and
  pending incoming (default 1000), `max_connections_per_peer` (default 4) — and `max_streams`
  concurrent streams per connection and protocol (default 16). Media additionally caps this
  at two per connection and sixteen inbound requests across the whole node. A shared codec
  permit is acquired before reading the body and held until the response or stream drop;
  excess streams close without a `RATE_LIMITED` response. Empty response streams are reported
  as transport failures rather than decoded as `STATUS_UNSPECIFIED`. Frames are capped at
  1 MiB + 64 KiB, or 6 MiB + 64 KiB on `/dyapp/media`. Request-response streams time out
  after 20 seconds, leaving time for the acceptor's 10-second write deadline.
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
profiles.db  profiles(peer_id PK, record BLOB, live, last_seen)
             record: SignedRecord (protobuf), latest version or tombstone (live = 0)
messages.db  envelopes(seq PK, mailbox BLOB, id BLOB, record BLOB, size, expires_at, class,
                       token_key BLOB, via BLOB, source_group BLOB, UNIQUE (mailbox, id))
             record: the signed envelope as received; class: 1 signal, 0 message
             token_key: the write token key, NULL for a tokenless envelope
             via, source_group: 8-byte eviction labels (the tokenless pool)
             indexes on (mailbox, class, seq, size), expires_at,
               (mailbox, class, via, source_group, seq, size) WHERE token_key IS NULL,
               (mailbox, token_key, class, seq, size) WHERE token_key IS NOT NULL
             tokenless_mailbox(mailbox, class, bytes, PK (mailbox, class)), index (class, bytes)
             tokenless_total(class PK, bytes): tokenless bytes, kept by insert/delete triggers
             revoked(mailbox, token_key, expires_at, PK (mailbox, token_key)), index expires_at
             revokes(seq PK, mailbox, record BLOB, expires_at, UNIQUE (mailbox, record)),
               index expires_at; record: the signed revoke, sent by repair
media.db     owners(owner PK, version, last_seen, record BLOB)
             attachments(slot PK, release, owner, expires, created, record BLOB)
             blobs(owner, hash, size, cost, PK (owner, hash))
             manifests(hash PK, record BLOB)
             shards(hash, idx, data BLOB, PK (hash, idx))
             released(release PK, expires), index expires: release hashes refused until expires
             record: signed keep/attachment (nullable on legacy rows), or manifest protobuf
             cost: billed ownership bytes; legacy NULL means size × 5
             shards.data: at most 1 MiB per index; physical usage sums shards and manifests
             owner: hex SHA-256 of the identity key; size NULL = listed, not yet put
data/aa/bb/<hex hash>  the blob
profiles-idx.db  profile(key PK, topic, version, search fields, place, r), interest(key, interest)
                 record(key PK, record BLOB), key(key PK, topic, version, deleted, updated,
                      heartbeat, beat_day, beats, first_seen, bytes)
                 place_count(country, place, n), meta(name PK, value): `created`
                 search role only; key = SHA-256 of the identity key; disposable
```

A `messages` table left by an older version is not read; it can be dropped by hand.
`put_envelope` sums the `size` of the mailbox's envelopes of the same class on each put, from
the covering index, in one immediate transaction. A message over the quota stores nothing; a
signal over it first deletes the mailbox's tokenless signals by the pool rule, then, only when
none remain, the oldest signal of the token key holding the most bytes. The pool rule takes the
`via` holding the most bytes, in it the `source_group` holding the most, and that group's oldest
envelope (ties go to the label holding the oldest envelope), until the new one fits; when the
new envelope would itself be the victim, the put deletes nothing and drops it with `OK`. With a
pool, tokenless messages count against the pool by the same rule and tokened ones alone against
the quota; the service passes the pool only with `limits.require_write_tokens`.
`evict_tokenless` applies the rule store-wide: it trims the (mailbox, class) holding most
tokenless bytes until it is no longer the largest, until the tokenless total is within a limit;
the service calls it after each stored tokenless envelope.

Each file has one connection behind a mutex. `storage::open` keeps the format version in
`PRAGMA user_version` and runs the missing steps in place on open, each in its own transaction:
`profiles.db` 1 adds `last_seen`, `media.db` 1 adds the owner `last_seen`, the attachment
`created`, the signed `record` columns, `cost` and the replica tables, `media.db` 2 the purge
counter, `media.db` 3 the `released` table; `messages.db` 1 adds the
envelope `class` column and the `(mailbox, class, seq, size)` index in place of `(mailbox, seq)`,
`messages.db` 2 the `token_key`, `via` and `source_group` columns, their partial indexes, the
tokenless counter tables and triggers, counting existing rows as tokenless, `messages.db` 3 the
`revoked` and `revokes` tables;
`deny.db` is at 0. The first start after each `messages.db` step builds the indexes, O(n) once
(seconds on a large file). An older `dyappd` refuses the newer `messages.db`; to roll back, move
it aside, losing at most one TTL of undelivered mail, part of which mailbox repair returns. A file from before versions counts as 0 and its steps skip changes already there. A
file newer than the binary fails the start with `<file> has store format N, this dyappd knows up
to M; run a newer dyappd`. Rebuilding a file next to the old one is planned
([Principles](#principles)).

**Compaction:** `BootstrapStore::cleanup_expired` deletes expired envelopes; `dyappd` calls it every hour, then `expire_profiles` deletes profiles and tombstones whose owner was last
seen more than `limits.profile_ttl_days` ago (`last_seen` column, added on upgrade with the
upgrade time for existing rows), and the search index drops keys not heard from in that time,
at most 4096 per run; under a full quota `evict_profiles` and `evict_envelopes` delete the oldest rows by rowid ([resource guards](#resource-guards)). `INSERT OR REPLACE` gives a republished profile a new, highest rowid, so rowid order is publish order.

**Maintenance:** every `maintenance.interval_minutes` (default 60) `dyappd` calls
`BootstrapStore::maintain` and, on a search node, `SearchIndex::maintain`; one store at a time
under its lock, each frees at most `maintenance.vacuum_pages` free pages (default 2048 = 8 MiB)
with `incremental_vacuum`, checkpoints the WAL with `TRUNCATE` and runs `PRAGMA optimize`, then
logs the free pages left. There is no full `VACUUM`: a large freelist shrinks over several runs,
and requests to that store wait for one run only. A rebuilt search index generation gets its
statistics at the first run after the switch. Not measured yet: fragmentation under load, and
whether time-bucketed message files beat deletes.

SQLite settings for the first release; none changes without a benchmark that shows a gain:

| Setting | Value | Decision |
|---|---|---|
| `page_size` | 4096 (SQLite default) | Kept. SQLite recommends it for most applications; rows are small (a profile, an envelope). Changing it on an existing WAL file needs `journal_mode = DELETE` and a full `VACUUM`, a rebuild of every store. 32 KiB would be benchmarked first. |
| `encoding` | UTF-8 (SQLite default) | Kept. Already UTF-8; settable only before the first table, so an explicit pragma changes nothing. |
| `auto_vacuum` | `INCREMENTAL` | Kept. Set before the first table; a file created without it stays `NONE`. |
| `incremental_vacuum` | `maintenance.vacuum_pages` (2048) per store and run | Kept. A fixed 10 pages would free 40 KiB an hour, far below the delete rate. |
| `ANALYZE` | `PRAGMA optimize` each run | Kept; the run now covers the search index, so a rebuilt generation is analysed too. |
| `journal_mode`, `synchronous` | WAL, `NORMAL` | Kept. A crash loses at most the last commits, never the file. |

`put_profile` holds a lock across read-compare-write, so two concurrent uploads for one peer cannot both win.

## Configuration

`NodeConfig` (`rust/bootstrap/src/config.rs`) is read by `dyappd`: defaults, then a TOML
file (`--config`, else the first of `./dyappd.toml`, `~/dyappd.toml`, `/etc/dyappd.toml`),
then `DYAPPD_<SECTION>_<KEY>` variables, then `--<section>.<key> <value>`
options (unknown ones refused; `--help` prints them from `rust/bootstrap/dyappd.toml`, which a
test keeps equal to the defaults). Every field has a default and
unknown keys are logged and ignored, so configs work across upgrades and rollbacks. Keys:
`listen`, `external`, `seeds` ([joining](p2p-networking.md)), `roles`, `storage.dir` (every store
in it, media blobs in `<dir>/data`; the removed `storage.{profiles,messages,media}` are refused),
`storage.key` (the node key file; empty: the unit's `node.key` credential, else `<dir>/node.key`),
`limits.{message_ttl_hours,requests_per_second,media_requests_per_second,media_per_owner_mb,attachment_retention_hours,media_cache_minutes}`,
`limits.{profiles_max_mb,messages_max_mb,signal_max_kb,media_max_mb,min_free_mb,bytes_per_second}`,
`limits.{mailbox_pool_kb,tokenless_share_pct,require_write_tokens}` ([tokenless pool](#mailbox-envelope)),
`limits.{max_connections,max_connections_per_peer,max_connections_per_ip_group,max_streams,max_memory_mb}`
([Resource guards](#resource-guards)),
`limits.{ip_group_requests_per_second,ipv4_prefix,ipv6_prefix,coarse_ipv4_prefix,coarse_ipv6_prefix,sender_puts_per_second,strikes_to_ban,ban_minutes}`
([Rate Limiting](#rate-limiting)), `maintenance.{interval_minutes,vacuum_pages,io_mb_per_s}`,
`network.{name,id_pow_bits,distinct_outbound_groups,guard_dials,deny_dial}` ([P2P networking](p2p-networking.md)),
`network.storage_trust_minutes` ([Rate Limiting](#rate-limiting)),
`network.{share_deny_list,accept_deny_lists}` ([Deny-list exchange](#deny-list-exchange)),
`turn.{urls,secret,secret_file,credential_minutes}` (`secret_file` is read into `secret` before validation) ([TURN](#turn-credentials)),
`search.{max_profile_bytes,max_bytes,newcomer_days,newcomer_share,max_scan_rows,topics,puzzle_wait_ms,max_puzzle_bits}` ([Search](#search), checked only with
the role); the
example and startup checks are in [operator manual](../manuals/bootstrap.md#configuration). `dyappd`
starts the libp2p node ([P2P networking](p2p-networking.md)) in `Mode::Auto` with the stores open
and serves the protocols in [Served protocol](#served-protocol). The `store`, `media`, `turn`
and `search` roles are accepted, the last three only together with `store`. On start it dials its
anchors, then fills the routing table from the peer cache; the seeds follow
([P2P networking](p2p-networking.md#current-code)).

Planned: the remaining resource guards, store retention.

## Deployment Model

Every node is independent: there is no cluster, leader or shared storage. Nodes find each other
through the DHT ([P2P networking](p2p-networking.md)). The client uploads once and the accepting
node sends profile, mailbox and media replicas to the holders of their keys, so losing a node loses
only the data whose other replicas are gone too. Mailboxes are repaired when their device watches,
profiles on a publish or heartbeat, media on the owner's signed `keep`.
A single node (development, or a network of one) keeps the only copy. The network test runs
three nodes, stops a holder during a newer publish, restarts it and checks heartbeat-driven repair.

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
known peers, requests answered and failed with a node error since the last run, connections
dropped after identify because the peer belongs to another network (`foreign_peers`; a peer with
two connections counts twice), stored bytes of
profiles, messages and media, profile proof-of-work checks (`pow_checks`) and forgeries
(`pow_forged`), search newcomers evicted by the quota (`newcomer_evictions`), the search
index phase (`search_phase`: `off`, `serving`, `rebuilding` or `refilling`), its active
version (`search_index_version`) and rebuild postpones, overflows, forced drops and forced
stops since start (`rebuild_postpones`, `rebuild_overflows`, `rebuild_drop_refills`,
`rebuild_stop_the_worlds`), capped replica
sends (`replica_dropped`), refused outbound dials (`throttled_dials`, and `denied_dials` for `network.deny_dial`), and per-protocol `wire_bytes`; plus refusals per
guard and refused connections ([Resource guards](#resource-guards)). The same run and every SIGHUP
log `node state`: replica, lookup and inventory requests still pending, the largest per-connection
map (challenge nonces, connection IPs, dialed peers) against the open connections, mailbox watches and
peers with a trust score. An idle node drains to no pending request and no entry of a closed
connection; a count that idling does not lower is a leak. `dyappd status` prints whether
the node answers, what the stores hold and the metrics totals; there is no admin HTTP API.

`wire_bytes` reports protobuf frame bytes since the previous status line as
`node=in/out profile=in/out mailbox=in/out push=in/out media=in/out search=in/out inventory=in/out digest=in/out gossip=in/out other=in/out keypackage=in/out`.
A received frame counts only after successful decoding; a sent frame counts only after a full
write. Malformed/oversized reads and partial/failed writes are excluded. `digest` is reserved
but not currently served; unknown protocols count as `other` only when handled by this codec.
Gossip counts each delivered application payload before validation and each successful local
publish once, excluding transport fan-out, duplicates suppressed by libp2p, and control traffic.
Kademlia, identify, AutoNAT, transport overhead and encryption framing are not measured here.
Each status line reports the change since the previous one; the counters behind it never reset
(see [Metric families](#metric-families)). The snapshot is not globally atomic. See the
[operator manual](../manuals/bootstrap.md#status-and-metrics).

`dyapp-p2p-net` provides `WireBytes` / `WireCounts` for interval payload-byte accounting.
The fixed slots are node, profile, mailbox, push, media, search, inventory, digest, gossip,
other and keypackage; counts display as `name=in/out`. Digest is reserved and has no served protocol yet.
Counters only grow: `totals()` reads them since start and `take()` returns the change since the
previous take, so concurrent increments fall into this or the next interval and the snapshot
across counters is not globally atomic. They exclude transport overhead and gossip control
traffic, duplicate delivery and forwarding fan-out.

### Metric families

`dyapp_bootstrap::metrics` keeps process-lifetime counters and renders them in the Prometheus
text format 0.0.4, served by the [metrics listener](#metrics-listener); the hourly status lines
take their own deltas and never reset them. Every name starts with `dyapp_`; label values come
from fixed tables, never from peers, keys, hashes or addresses, so the series count does not grow
with traffic.

| Family | Type | Labels |
|--------|------|--------|
| `process_start_time_seconds` | gauge | — |
| `requests_total` | counter | `protocol`: node, profile, mailbox, media, search, inventory, digest |
| `request_outcomes_total` | counter | `protocol`; `outcome`: answered, timeout, closed, unsupported, omitted, io |
| `wire_bytes_total` | counter | `protocol`: the `WireBytes` slots; `direction`: in, out |
| `pow_checks_total`, `pow_forged_total` | counter | — |
| `refused_total` | counter | `guard`: disk_profiles, disk_messages, disk_media, memory_rate_limited, memory_dropped, traffic_bytes_per_second, unknown_endpoint |
| `connected_peers`, `connections` | gauge | — |
| `search_index_version` | gauge | — (search nodes) |
| `search_phase` | gauge | `phase`: refilling, rebuilding, serving; 1 for the current one (search nodes) |
| `search_rebuild_events_total` | counter | `event`: postpone, overflow, drop_refill, stop_the_world (search nodes) |
| `search_newcomer_evictions_total` | counter | — (search nodes) |
| `snapshot_timestamp_seconds` | gauge | — ; 0 before the first storage read |
| `storage_read_ok` | gauge | `store`: profiles, messages, media (configured stores only) |
| `storage_used_bytes`, `storage_free_bytes` | gauge | `store`; absent for a store whose last read failed |

Requests are counted in the swarm loop from the request-response events: a request when it
arrives and its outcome when the reply is sent or libp2p reports the inbound failure, so a reply
sent long after the request (a replica answer, a store worker) counts once. A request dropped
unanswered, by a guard or under memory pressure, ends as `omitted`. Media bytes served are
`wire_bytes_total{protocol="media",direction="out"}`. The connection gauges change on
connection events. Storage gauges come from a snapshot read every 5 minutes on the blocking
pool, off the swarm loop; rendering reads only atomics, that snapshot and the search index
state, takes no store lock and changes nothing, so repeated scrapes are safe. A failed read
sets `storage_read_ok` to 0 and drops the size samples rather than reporting zero.

### Metrics listener

`[metrics] port` turns on an HTTP/1.x listener in the node process; it is 0, and the listener
off, by default. The listener binds only the loopback addresses `[::1]` and `127.0.0.1` at that
port, because the endpoint has no authentication. An address the host cannot open, such as `[::1]`
without IPv6, is skipped with a warning; startup fails when the port is taken on either address
or neither opens. The listener and the 5-minute storage snapshots run only when the port is set;
they stop with the process.

It answers one request per connection and closes it. `GET /metrics` returns the families above
with `Content-Type: text/plain; version=0.0.4; charset=utf-8`; another method on that path gets
405, any other path, query string included, 404, a malformed request line 400 and a request head
over 8 KiB 431. At most 4 connections are served at once, others wait in the accept queue, and
a connection that has not sent its request and taken the reply within 5 seconds is closed. The
listener reads no request body and serves nothing else.

## Testing

Unit test coverage:
- Envelopes once per (mailbox, id), mailbox full, fetch limits and `more`, ack, expiry (`storage.rs`)
- Signed profile: version order, forged payload, tombstone hides and blocks older versions (`storage.rs`)
- Rate limiting (single key, separate keys)
- Replication codec encode/decode with a missing shard (in-process; no nodes involved)
- Store health check (`storage.rs`)
- Profile and node requests through `Service` (`service/tests.rs` and `service/tests/`): publish, get, stale with the
  stored record, forged, wrong-length ID, rate limit, `info`
- Mailbox through `Service`: put, forged and wrong-domain put, bad id, too large; fetch without
  a challenge, with another connection's nonce, signed as an ack, replayed; another key reads
  and acks only its own mailbox; ack and its replay
- `ProtoCodec` round trip over TCP between two swarms (`rust/p2p-net`)
- Media stream admission: third request on one TCP connection and seventeenth across nine
  connections fail without a response; accepted streams still answer. Codec tests prove that
  rejection reads no body bytes and that completed, malformed and dropped streams release permits.

- Sender replica cap (`node/tests.rs`): shared cross-protocol quotas, local exemption, dropped-count
  reset, media queue cleanup and no trust penalty. Mailbox repair over TCP delivers 75 envelopes
  as an admitted 50-envelope batch and the remaining 25 after refill; complete inventory sends nothing.

Protocol tests (`rust/bootstrap/tests/protocol.rs`): `node::run` on loopback TCP with libp2p
clients:
- Every request type: `info`, publish and get, mailbox put, challenge, fetch and ack
- A challenge from one connection is `DENIED` on another and still valid on its own
- Bad input fails only its request: undecodable protobuf and a request over its protocol's
  wire limit get the
  stream closed without a reply (a typed client reports a transport failure), an unknown or empty
  variant gets `UNSUPPORTED`, and the node then answers the next client
- Rate limit over the wire: `RATE_LIMITED` past the burst, `info` still answered
- Valid mailbox/profile/media replicas and media shards: weak transport-ID PoW gets `DENIED`,
  qualifying node keys get `OK`. Exhausted peer and IP-group buckets keep the same connection
  open even with a one-strike ban threshold; `info` still answers.
- Search between two search nodes: a profile published to one is found by a query on the other
  (`search_profile_spreads_between_nodes`); eight queries at once each get `OK` or `RATE_LIMITED`
  with `retry_after` 1, and `info` is answered within a second (`search_queries_do_not_hold_up_the_node`, timing-based)

Integration tests (`tests/integration_tests.rs`, in-process, no network):
- Replication codec with one lost shard (`test_replication_fault_tolerance`)


The network test (`tests/scripts/network-test.py`) runs three `dyappd` instances, all with
roles `store`, `search` and `media` (only the first also serves TURN), and drives them with `dyappd`'s client commands: `info` over TCP and QUIC, publish
over TCP and get over QUIC with identical bytes, `STALE` on replay; a mailbox put over TCP and
fetch over QUIC, a stranger's key reads nothing, ack empties it, a watching fetch gets a push; a
media keep, put and get; 512 KiB whole-copy and 3 MiB sharded uploads readable on all three
selected holders over TCP and QUIC; the local run loses a node holding at most four shard
indices and reads the 3 MiB blob from both survivors; placement on DHT holders; single-upload profile and mailbox replication onto all three nodes, ack forwarding,
repair and the rate limit. CI runs it with `--local`, which also stops a node and checks replicas skip it.
The local run checks the assembled cache of a 3 MiB blob, removes the temporary holder's
shard and manifest rows, then reads the blob again to verify later ranged pieces use the cache.
An hourly expiry unit test verifies idle cache entries are removed while recent ones remain.
The local run also deletes a temporary whole-copy file and a shard row on a reachable holder,
refreshes keep on another node and verifies both are restored. It also ages media fixture rows,
refreshes one keep through its signed protocol request,
then restarts the media node: startup cleanup removes the expired owner's blob while the
refreshed owner's blob remains readable. The same local suite runs inside the prepared dev
Docker image, with temporary storage and reports under `/tmp/ai`. It also stops the other
remaining node and verifies that profile and mailbox puts answer `FULL` without a second holder.
The local run also adds a mailbox address to the first node's deny list and, after SIGHUP,
gets `REFUSED` for a put to it; the third node accepts deny lists and, within a maintenance
minute, `dyappd deny received` shows the first node's signed list with that entry. On the third
node `127.0.0.1/24` is refused while listed and served once removed. Restarted nodes keep their
peer ID. After the suite, SIGHUP on the first and third node must show `node state` drained:
nothing pending, no watch, no more per-connection entries than open connections.
`network-test.py --plateau BIN_DIR` (`rust-check.sh memory`, part of `make test-integration`, not
CI) repeats the suite's mailbox and media load on three release nodes for 5 minutes, without
the flood limit, and fails when a node's resident memory peak in the second half exceeds the first
half's by more than 10 %.
The Docker network compose mounts the current checkout and the dev build volume read-only;
run `make test` first to build the current `dyappd` binary.

## Limitations & Future

**Current limitations:**
- App clients are not wired to the protocol yet; nodes replicate mailbox/profile writes, forward acks and repair mailbox gaps only when a device watches its mailbox
- Search expiry drops at most 4096 silent keys an hour
- No audit logging

Planned changes: see [Target Design](#target-design--planned).
