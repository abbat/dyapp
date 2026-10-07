# 0003. Public signed profile, end-to-end encrypted private data

- **Status:** Accepted
- **Date:** 2026-10-07

## Context

Search on bootstrap and index nodes needs readable fields, and a profile that strangers cannot read
cannot be used for matching. Conversations, on the other hand, must stay private from the nodes
that store and relay them.

## Decision

- **The profile is public.** Every field a user publishes is plaintext and visible to anyone,
  including bootstrap and index nodes. There is no per-field private flag: a user who does not want
  a field public does not fill it in. This applies to sensitive optional fields (for example
  mental-health indicators) as well.
- **Location is a place name, never coordinates:** a city or district the user picks within their
  country, or none. Search matches the place exactly and must work without it.
- **The profile is tamper-proof:** every profile version is signed with the owner's Ed25519
  identity key ([ADR 0004](0004-identity-keys.md)). Readers and index nodes reject unsigned or
  badly signed profiles.
- **Everything else personal is end-to-end encrypted** ([ADR 0005](0005-openmls-end-to-end-encryption.md)):
  messages, media, call signalling, user settings. Bootstrap and relay nodes see only ciphertext
  and routing metadata.

## Consequences

- Index nodes can search plainly; no encrypted index is needed.
- A published field cannot be taken back reliably once replicated
  ([ADR 0011](0011-best-effort-deletion.md)); the UI must say so before publishing, especially for
  location and sensitive fields.
- Signature checks are required on every read path.
- Implemented on the bootstrap only: it accepts signed protobuf profiles and verifies the signature
  before storing ([bootstrap](../architecture/bootstrap.md#signed-profile)). No app reads profiles
  yet, and private data is not encrypted ([privacy](../security/privacy.md)).
