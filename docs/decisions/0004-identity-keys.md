# 0004. Long-lived Ed25519 identity key

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

A user needs one identity that survives key changes and new devices. Keys of a first contact come
from untrusted bootstrap nodes, so a man in the middle is possible.

## Decision

- **Peer ID = hash of a long-lived Ed25519 identity key.** The identity key signs only device
  certificates ([ADR 0005](0005-openmls-end-to-end-encryption.md)) and profiles
  ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)). It encrypts nothing and never
  takes part in a transport handshake ([ADR 0006](0006-transport-keys-separate-from-identity.md)).
- **First contact is verified out of band** with a safety number or QR code; until then the key is
  trusted on first use and the UI shows it as unverified.
- **No private-key sync through any cloud or server.**
- **A new device is linked from an existing device**: it receives keys and history from it and
  joins the user's MLS groups.

## Consequences

- Losing every device loses the identity and history; there is no recovery service.
- Device linking and safety-number UI must be built on every platform.
