# 0012. Interactions between users are private P2P signals

- **Status:** Accepted
- **Date:** 2026-10-06

## Context

With a public signed profile and end-to-end encrypted private data
([ADR 0003](0003-public-signed-profile-encrypted-private-data.md)) there is no server that could
keep likes, view history or matching preferences private, or enforce rules such as an age gate.

## Decision

- **Matching uses public fields only.** Search and filters may use any published field, sensitive
  ones included (for example mental-health indicators): if a field exists, it can be searched.
  Search preferences, pass history and other settings stay on the device.
- **Income** is a free "from–to" range with no currency and no protocol-defined brackets. The
  profile gets a **country** field; income is compared only within one country.
- **A like is a private P2P message** to the liked user, end-to-end encrypted like any message
  ([ADR 0009](0009-message-delivery-and-storage.md)). It is not stored in the profile or published
  on bootstrap. The recipient sees who liked them after delivery; a match is a mutual like. Undoing
  a sent like sends an encrypted unlike signal so both sides remove the active match when it
  arrives. Offline delivery can delay this, and an earlier notification cannot be erased.
- **A profile view is an optional P2P signal** from the viewer to the viewed user. Whether to send it
  is a local setting of the viewer, not a profile field. There is no reciprocity: turning sending
  off does not stop receiving. Undelivered signals are stored on bootstrap like messages.
- **Online presence** is visible to whomever the user chooses; the setting is local and presence is
  sent only to the allowed peers.
- **Messages to non-matches** are allowed with rate limiting and one pending message until the
  recipient replies.
- **Multiple profiles are separate identities:** each has its own identity key
  ([ADR 0004](0004-identity-keys.md)), they are not linked to each other, and each has its own
  conversations.
- **Birth-date parts are self-declared.** The planned client requires an 18+ attestation and,
  when a year is given, checks the youngest possible age; 30+ is the target audience,
  not a rule. Nothing can enforce either on the network.
- **Moderation in v1 is local:** block and mute on the device, and a node operator may refuse to
  store any content. Network-wide weighted voting comes only after the Sybil-resistance work
  ([ADR 0008](0008-sybil-and-eclipse-defences.md)).

## Consequences

- Likes, views and other signals are ordinary encrypted mailbox envelopes marked only as
  "signal", so a node cannot tell a like from a view. They have their own per-mailbox quota with
  the oldest evicted first, so the likes of a popular profile never block its messages; a
  recipient offline for long may lose the oldest signals.
- A user with several profiles gets no cross-profile features: no shared inbox, no switching
  conversations between profiles.
- On desktop, each planned local identity slot chooses its own key backend; the
  [storage layout](../development/desktop-identity-storage.md#multiple-identity-layout)
  preserves an existing single-profile key in place. This does not link the profiles on the
  network.
- Age, income and every other field are claims, not facts; the UI must not present them as
  verified (verification is low priority).
- The bootstrap implements the signal class and per-mailbox quota
  ([bootstrap](../architecture/bootstrap.md#limits-and-abuse)). The Rust client implements the
  interaction codec, local state and encrypted mailbox send/receive path. App UI and own-device
  sync remain planned; the known outbox atomicity gap and replay rules are in the
  [private interaction design](../design/private-interactions.md).
