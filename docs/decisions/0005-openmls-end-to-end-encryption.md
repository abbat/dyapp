# 0005. End-to-end encryption with MLS (openmls)

- **Status:** Accepted
- **Date:** 2026-10-07

## Context

Private data ([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)) needs end-to-end
encryption with forward secrecy and post-compromise security. A key schedule written by us is where
cryptographic bugs come from. `rust/messaging` declares `chacha20poly1305` and encrypts nothing.

**openmls** implements MLS ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)) under the MIT
licence. SRLabs audited it in 2025–2026; the one high-severity finding was fixed in 0.8.1 and 0.7.3
([report](https://blog.openmls.tech/SRL-OpenMLS_security_assurance_assessment.pdf),
[summary](https://blog.phnx.im/openmls-independent-security-audit/)).

## Decision

Encrypt everything private with **MLS via the `openmls` crate**:

- Each conversation is an MLS group; a one-to-one chat is a group of the two users' devices.
- Ciphersuite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (0x0001, mandatory to implement).
- **Credentials.** Every device has its own MLS signature key (RFC 9420 requires unique keys per
  member), certified by the user's Ed25519 identity key. A client accepts a member only if the
  certificate chains to the Peer ID ([ADR 0004](0004-identity-keys.md)).
- **KeyPackages:** each device publishes a few single-use KeyPackages plus a last-resort one to the
  bootstrap network ([ADR 0007](0007-open-bootstrap-network.md)); a first message starts the group
  from one of them.
- **Video** takes its per-call frame key from the MLS exporter of the chat's group
  ([ADR 0013](0013-video-calls-one-to-one.md)).

## Consequences

- MLS commits are ordered per group: two devices committing at once need a retry, and offline
  delivery ([ADR 0009](0009-message-delivery-and-storage.md)) must carry commits as well as
  messages.
- Consumed KeyPackages must be replenished; an exhausted device falls back to its last-resort
  KeyPackage, which weakens forward secrecy for that first message.
- Not implemented: `rust/messaging` has no MLS; `encryption.rs` is a stub.
