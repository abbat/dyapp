# Protobuf Schema

> **Status:** the schemas in `proto/` are compiled into the Rust crates
> ([code generation](../development/protobuf-codegen.md)). `identity.proto` and `profile.proto`
> are the signed profile used today. `node.proto` defines the planned libp2p node protocol;
> no node serves it yet, and offline messages are still JSON over the bootstrap REST API
> ([bootstrap](bootstrap.md#data-model)).

## Files

| File | Package | Messages |
|------|---------|----------|
| `proto/identity.proto` | `dyapp.identity` | `SignedRecord` |
| `proto/profile.proto` | `dyapp.profile` | `Profile` |
| `proto/node.proto` | `dyapp.node` | `Status`, `Role`; requests and replies of `/dyapp/node`, `/dyapp/profile`, `/dyapp/mailbox`, `/dyapp/mailbox-push` |

The files are the source; comments in them define each field. Schemas for `/dyapp/signal`,
`/dyapp/media`, `/dyapp/search`, `/dyapp/inventory` and `/dyapp/turn` are planned and are added
together with their roles ([protocol](bootstrap.md#protocol)).

## Signed records

`SignedRecord` carries the signer's Ed25519 public key, a payload and a signature over
`domain || payload`. The domain label (for example `dyapp/profile/v1\0`) names what is signed, so
a signature for one kind cannot be replayed as another. The payload is signed and stored exactly
as received, so a node keeps fields it does not understand.

| Payload | Domain | Signed by |
|---------|--------|-----------|
| `dyapp.profile.Profile` | `dyapp/profile/v1\0` | identity key |
| `dyapp.node.Envelope`, `Fetch`, `Ack` | planned | device key |

## Profile

Every field is public ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)):
anything private is end-to-end encrypted and never sent as a profile field. Empty strings, unset
optionals and an age of 0 mean "not published". `version` orders updates (the highest validly
signed version wins); `deleted` marks a tombstone with every other field empty. Tag 5 held
coordinates and is reserved. See the [bootstrap signed profile](bootstrap.md#signed-profile).

## Node protocol

One request message per protocol, each a `oneof`; every reply carries a `Status`. Identifiers
are raw bytes: the peer ID of an identity is SHA-256 of its public key, a mailbox address is
SHA-256 of the device public key (32 bytes each).

- **`/dyapp/node`**: `info` returns the node's roles and limits; zero means "no such limit" or
  "role not served".
- **`/dyapp/profile`**: `publish(SignedRecord)` and `get(peer_id)`. A publish whose version is not
  newer gets `STATUS_STALE` with the stored record.
- **`/dyapp/mailbox`**: `challenge` returns a nonce bound to the connection; `fetch` and `ack` are
  signed by the mailbox's device key and carry that nonce. `put` stores an `Envelope` once per
  random 16-byte id, so it needs no nonce. A node forwards a signed ack to the other replicas
  verbatim; they check the signature and mailbox address, not the nonce.
- **`/dyapp/mailbox-push`**: after a fetch with `watch`, the node pushes new envelopes over the
  same connection; the device still acknowledges with an ack.

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
