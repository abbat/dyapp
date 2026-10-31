# 0004. Long-lived Ed25519 identity key

- **Status:** Accepted
- **Date:** 2026-10-31

## Context

A user needs one identity that survives key changes and new devices. Keys of a first contact come
from untrusted bootstrap nodes, so a man in the middle is possible.

## Decision

- **Peer ID = hash of a long-lived Ed25519 identity key.** The identity key signs only device
  certificates ([ADR 0005](0005-openmls-end-to-end-encryption.md)) and profiles
  ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)). It encrypts nothing and never
  takes part in a transport handshake ([ADR 0006](0006-transport-keys-separate-from-identity.md)).
- **First contact is verified out of band** with a safety number or QR code; until then the key is
  trusted on first use and the UI shows it as unverified. The safety number and QR code are
  derived from both Peer IDs.
- **The identity key never changes**: a new key is a new Peer ID and a new contact. What changes
  is the device list in the profile, so every chat with the contact shows a new device, and a
  verified contact loses its verified mark until it is checked again.
- **No private-key sync through any cloud or server.** A user may manually make an offline copy
  of a file-backed desktop identity folder. The app does not export keys from platform credential
  stores or manage the security of that copy.
- **A new device is linked from an existing device**: it receives keys and history from it and
  joins the user's MLS groups.

## Consequences

- Without a usable offline copy, losing every device loses the identity and history; there is no
  recovery service. A file-backed desktop copy can restore the key and local data present in
  the copied folder. Copying app files does not recover keys stored in a platform credential
  store.
- Device linking and safety-number UI must be built on every platform.
