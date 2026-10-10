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
| `proto/node.proto` | `dyapp.node` | `Status`, `Role`; requests and replies of `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push`, `/dyapp/media` |

The files are the source; comments in them define each field. Schemas for `/dyapp/search`
and `/dyapp/inventory` and the `Envelope.class` field are planned and are added
together with their roles ([protocol](bootstrap.md#protocol)).

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
signed version wins); `deleted` marks a tombstone with every other field empty. Tag 5 held
coordinates and is reserved. `photos` link `/dyapp/media` blobs by SHA-256: each `Photo` is the
full image as ordered blobs plus one small `thumbnail` blob for lists and search results. See
the [bootstrap signed profile](bootstrap.md#signed-profile).

## Node protocol

One request message per protocol, each a `oneof`; every reply carries a `Status`. Identifiers
are raw bytes: the peer ID of an identity is SHA-256 of its public key, a mailbox address is
SHA-256 of the device public key (32 bytes each).

- **`/dyapp/node`**: `info` returns the node's roles and limits; zero means "no such limit" or
  "role not served".
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
  `put` sends a listed blob unsigned; `get(hash)` returns it.
  `attach` is a signed chat attachment: blob hashes, the SHA-256 of a release secret and a
  creation time; `release(secret)` drops it.

## FFI boundary

Protobuf types do not cross the FFI. `rust/ffi` exports UniFFI types; see the
[FFI contract](ffi-design.md).

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
