# Protobuf Schema

> **Status:** two draft schemas exist in `proto/`, and **no Rust crate compiles
> or uses either of them**. The signed profile is Protobuf, but its messages are
> `prost` derives in Rust (`rust/identity`, `rust/profile`), not `.proto` files.
> Offline messages are still JSON over the bootstrap REST API
> ([bootstrap](bootstrap.md#data-model)). This page describes what the `.proto` files say, where they disagree with each
> other and with the code, and the rules that apply once one becomes the wire
> format. Code generation is covered in
> [Protobuf code generation](../development/protobuf-codegen.md).

## Which schema is canonical

Neither, yet. Choosing one is open work.

| File | Package | Messages | Referenced by |
|------|---------|----------|---------------|
| `proto/core_messages.proto` | `dyapp.core` | `MessageEnvelope`, `Profile`, `QueuedMessage`, `CRDTDocument`, `ProfileQuery`, `FilterClause`, `ProfileQueryResult`, `EncryptedProfile` | nothing |
| `proto/messages.proto` | `dyapp` | `UserProfile`, `ChatMessage`, `SyncRequest`, `SyncResponse`, `PeerAnnouncement`, `TurnRequest`, `MatchEvent` | `scripts/regenerate-proto.sh` (not wired into any build) |

`rust/identity` and `rust/profile` use `prost` derive macros directly, and `rust/bootstrap` encodes
and decodes their types; no crate has a `build.rs` or code generated from `proto/`.

The two schemas model the same concepts differently:

| Concept | `core_messages.proto` | `messages.proto` |
|---------|-----------------------|------------------|
| Chat message | `MessageEnvelope`: `sender_id`/`recipient_id`, `bytes encrypted_payload`, `bytes signature` | `ChatMessage`: `from_user_id`/`to_user_id`, `bytes content`, `string signature` |
| Profile | `Profile`: 22 typed fields, per-field `encrypted_*` strings | `UserProfile`: `name` + opaque `bytes profile_data` (CRDT state) |
| Ordering clock | `string lamport_clock` (commented "Vector clock") | `int64 lamport_clock` in `SyncResponse`; `string state_vector` in `SyncRequest` |
| Signature type | `bytes` | `string` |

A Lamport clock is a single counter and a vector clock is a per-peer map; a
`string` field holding either is unspecified. The chosen schema must name one
and give its encoding.

## Wire types today (code)

| Data | Serialization | Defined in | Timestamp unit |
|------|---------------|-----------|----------------|
| Offline message (`MessageBlob`) | JSON (`serde_json`), REST + sled | `rust/bootstrap/src/storage.rs` | Unix **seconds** (`as_secs()` in `api.rs`) |
| Signed profile (`SignedRecord` wrapping `Profile`) | Protobuf (`prost` derives), REST `application/x-protobuf` | `rust/identity`, `rust/profile` ([bootstrap](bootstrap.md#signed-profile)) | none; ordered by `version` |
| FFI types | UniFFI records/objects, no Protobuf | `rust/ffi` ([FFI contract](ffi-design.md)) | none |

`MessageEnvelope.timestamp` in `core_messages.proto` is commented "Unix millis".
That differs from the bootstrap's seconds, so the unit has to be reconciled
before the envelope is adopted. `Profile.updated_at`/`expires_at` and all
`messages.proto` timestamps state no unit.

## core_messages.proto (verbatim excerpts)

Snippets below are copied from the file with their field numbers. They are
not run through `protoc` as part of the docs; the file is the source.

```proto
syntax = "proto3";

package dyapp.core;

message MessageEnvelope {
  string message_id = 1;        // UUID
  string sender_id = 2;         // Public key hash
  string recipient_id = 3;      // Public key hash
  int64 timestamp = 4;          // Unix millis
  bytes encrypted_payload = 5;  // ChaCha20-Poly1305 encrypted
  bytes signature = 6;          // Ed25519 signature of (sender_id, timestamp, encrypted_payload)
  string lamport_clock = 7;     // Vector clock for ordering
}

message EncryptedProfile {
  string user_id = 1;
  bytes encrypted_data = 2;     // Full Profile encrypted with query key
  int64 updated_at = 3;
}
```

Encryption and signatures in the comments are target behaviour; neither is
implemented ([encryption status](../security/encryption.md)). The signature
covers `(sender_id, timestamp, encrypted_payload)` but no byte encoding of that
tuple is defined, so two implementations would not agree on what is signed.

`QueuedMessage`, `CRDTDocument`, `ProfileQuery`, `FilterClause` and
`ProfileQueryResult` are in the file as written; `FilterClause.value` is a
JSON-encoded string and `operator` is a free string (`"eq"`, `"range"`, `"in"`).

## Profile fields and privacy boundary

`Profile` is the field list from `core_messages.proto`. The **Visible to
bootstrap** column is what the schema, as written, would expose if a `Profile`
were sent in clear. A field named `encrypted_*` is a `string` that is supposed
to hold ciphertext. The schema does not say how it is encrypted, with which
key, or how it is encoded (base64, hex).

| Tag | Field | Type | Visible to bootstrap |
|-----|-------|------|----------------------|
| 1 | `user_id` | string | yes (lookup key) |
| 2 | `age` | int32 | yes (filtering) |
| 3, 4 | `location_lat`, `location_lon` | string | yes (filtering) |
| 5, 6, 9, 10, 11, 12, 15, 16, 17, 19, 20 | `encrypted_*` | string | ciphertext only (target) |
| 7, 8 | `has_kids`, `wants_more_kids` | bool | **yes: plain booleans** |
| 13 | `verified` | bool | **yes** |
| 14 | `interests` | repeated string | **yes**, unless each tag is encrypted (comment says "Encrypted tags", type gives no mechanism) |
| 18 | `wants_confidentiality` | bool | **yes** |
| 21, 22 | `updated_at`, `expires_at` | int64 | yes |

So the claim "everything except age/location/user_id is encrypted" does **not**
hold for this schema: four booleans and `interests` are plaintext. There are
two consistent designs. [ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)
settles the direction: the profile is public by design and signed, and everything private is
end-to-end encrypted and never sent as a profile field, so per-field encryption (option 2) is
not the target. The schema has not been changed yet:

1. **Whole-profile blob.** Send only `EncryptedProfile`: plaintext `user_id`
   and `updated_at` plus a single `encrypted_data` holding the serialized
   `Profile`. Searchable fields (age, location, verified) are then sent
   separately in clear. Per-field `encrypted_*` strings become unnecessary.
2. **Per-field encryption.** Every non-searchable field, booleans included,
   becomes `bytes` ciphertext with a defined AEAD and key. Then the bootstrap
   sees which fields are set, but not their values.

What the bootstrap actually receives today is the signed `Profile` from `rust/profile`: plaintext
public fields (age, country, place, kids, goals and so on) plus `version` and a `deleted`
tombstone flag, wrapped in a `SignedRecord`. Nothing in it is encrypted, matching
[ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md). See the
[bootstrap signed profile](bootstrap.md#signed-profile).

**Sensitive optional fields.** Mental-health indicators are an optional public
profile field like any other: published only if the user fills them in, then
readable by anyone ([ADR 0003](../decisions/0003-public-signed-profile-encrypted-private-data.md)).
No wire schema has such a field yet.

## FFI boundary

Protobuf types do not cross the FFI. `rust/ffi` exports UniFFI types
(`MessageService`, `VideoSession`, `KeyPair`, `UserProfile`); see the
[FFI contract](ffi-design.md). If Protobuf is adopted, it stays internal to
the Rust crates and the bootstrap protocol.

## Evolution rules (proto3)

These apply once a schema is adopted as the wire format:

- **Field numbers are permanent.** A field can be renamed but never renumbered
  or retyped incompatibly. When removing a field, add
  `reserved <tag>;` and `reserved "<name>";` so neither can be reused.
- **Defaults are indistinguishable from absence** for scalar fields in proto3
  (`0`, `""`, `false`). Use `optional` when "unset" must differ from "false",
  as it should for `has_kids`.
- **Unknown fields.** An older client parses a message containing new fields
  without error and, in current protobuf runtimes, keeps the unknown fields
  when it re-serializes. It does **not** understand them: it cannot read, show
  or validate data it has no field for. A change that needs older clients to
  act on new data needs a protocol version check (`PeerAnnouncement.protocol_version`
  in `messages.proto` is the only version field today).
- **Type changes.** Changing a field's type is a breaking change. Use a new
  field number or a new message. Moving `has_kids` from `bool` to encrypted
  `bytes` is a new field plus a `reserved` old tag, not an in-place edit.
- **Packages are part of the type name.** `dyapp.core.Profile` and
  `dyapp.UserProfile` are unrelated types on the wire, even when fields
  match.

**Example: adding a field.** Adding `string encrypted_language = 23;` to
`Profile` is wire-compatible: old readers skip tag 23. Old writers never set
it, so new readers see `""` and must treat that as "not provided".
