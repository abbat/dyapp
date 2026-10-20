# 0019. Device directory and KeyPackage rendezvous

- **Status:** Accepted
- **Date:** 2026-10-20

## Context

A sender needs the recipient's device mailbox addresses and MLS KeyPackages before any shared
MLS group exists. Neither is on the wire today. Multiple devices per identity are required in
the first client release. A package is normally single-use, but five eventually consistent store
replicas cannot atomically allocate one without a coordination protocol. The public profile
already has an identity signature and a monotonically increasing version.

## Decision

Add a public, identity-signed device directory to `Profile.devices` (field 24). It contains at
most 64 sorted, unique `DeviceCertificate` records, each signed by the identity key under its own
domain and binding one device Ed25519 mailbox key, a distinct MLS signature key and a generation.
The profile version governs additions and removals. Senders retain the highest version seen,
consult at least two replica points, and accept MLS members and KeyPackages only for certified
keys in that version. A missing current directory delays first contact.

A store-role `/dyapp/keypackage` service accepts device-signed, bounded MLS KeyPackage records and
serves them for a named mailbox. A record binds its exact bytes/hash, expiry and ordinary or
last-resort status. It is valid only with MLS ciphersuite 0x0001, a matching certified MLS leaf
signature key and a certificate in the current profile. The node fans publications
out to R = 5 mailbox-derived replica points. Each ordinary package is assigned to one slot by
its hash modulo five; only the current holder of that slot allocates it. Allocation is a local
durable consume-before-reply transaction. Other holders keep recovery copies but cannot
allocate that slot. A last-resort package is reusable only after ordinary allocation fails.

Each device keeps four ordinary packages per slot plus one last-resort package, refills below
eight ordinary packages or on its daily heartbeat, and rotates the last-resort one when online.
A package expires within 30 days. Nodes cap publications at 16 KiB each, 20 ordinary and one
last-resort package, and 512 KiB total per device; anonymous fetches are rate-limited by peer ID
and IP group. A higher-version profile that removes a device makes nodes refuse its publications
and stop serving its packages. Clients remove the device from MLS groups after learning that
profile. The exact request, response and storage schema land with the node implementation;
[bootstrap.md](../architecture/bootstrap.md#device-directory-and-keypackages) defines the
required behaviour.

## Consequences

- Device count, mailbox addresses and KeyPackage fetch activity are public to store nodes. A
  private directory would require a different first-contact discovery path.
- Five replicas improve availability but do not guarantee global single use. Holder churn,
  malicious holders or a lost consume reply can lead to duplicate allocation or wasted packages.
  The recipient rejects replayed Welcomes; senders retry with a fresh package. Last-resort reuse
  reduces first-message forward secrecy and can permit replay attempts
  ([RFC 9420 §16.8](https://www.rfc-editor.org/rfc/rfc9420#section-16.8)).
- A stale signed profile can delay revocation when all queried nodes withhold the newer one.
  Clients never roll back a version they have seen; removal cannot erase an already delivered
  message or a compromised device's local history.
- The directory, KeyPackage service and client are planned, not implemented. The separate node,
  client and revocation tasks must verify validation, race handling and multi-device delivery.
