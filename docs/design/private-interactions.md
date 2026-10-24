# Private interaction signals

This is the planned client contract for [private likes and views](../decisions/0012-private-p2p-interactions.md).
The bootstrap signal class and quota, the client plaintext codec and the local like/unlike ledger
exist; the own-event clock, view-day records, signal mailbox delivery and UI do not.

## Wire contract

An interaction is an MLS application message to the pair's verified device group, delivered to
each recipient device mailbox as an envelope with `class = signal`. The node sees the mailbox,
size and timing, but not the interaction kind. A single encrypted plaintext uses this binary v1
layout, in network byte order:

| Field | Size | Meaning |
| --- | ---: | --- |
| version | 1 byte | `1`; reject unknown versions without changing state. |
| kind | 1 byte | `1` like, `2` unlike, `3` view; reject other values. |
| event ID | 16 bytes | Random, unique for the sender identity; retries reuse it. |
| HLC timestamp | 8 bytes | Sender's persisted hybrid logical clock value. |

There are no optional fields or trailing bytes in v1. The sender and target identities come from
the verified MLS group and mailbox binding, not untrusted payload fields. The receiver accepts
only application messages from current verified group members in a group that identifies exactly
the intended two profile identities. Events from another device of the local identity update the
local side after device sync. The codec must not allow a signal to change the state of a third
profile. A group may contain several devices of either identity. A view uses the same layout but
never affects likes or matches.

The 26-byte plaintext fits the 256-byte short padding bucket. An introduction is a separate
one-message chat payload, not a v1 interaction event; its delivery policy belongs to chat design.
The [size guard](../architecture/bootstrap.md#mailbox-envelope) measured 638 bytes for a short
like and 894 bytes for up to 508 bytes of introduction plaintext if sent with the signal class,
both including a signed write token. The sender checks the *encoded envelope* against the node's
1 KiB signal limit before writing it; it never truncates ciphertext. The existing 1 KiB chat
padding cannot be used for signals.

## State and ordering

Each local profile stores, per peer identity, its own latest like state, the peer's latest like
state, an event-ID replay ledger, local pass history, and sent/received view-day records. A like
sets the sender's state to
liked; an unlike sets it to unliked. An unlike is an encrypted signal to the peer, including when
the earlier like was already sent. Both peers remove an active match when they apply it. A match
exists only when the latest known state in **both directions** is liked. Pass and undo-pass are
local changes; neither sends a signal. A view is sent only when the local setting allows it and
at most once per peer profile per UTC day. Receiving views never depends on that setting.

For each sender identity and peer pair, compare `(HLC timestamp, sender device ID, event ID)`
lexicographically. Only a greater like/unlike event replaces the stored latest event. Keep the
latest unlike as a tombstone: a delayed earlier like cannot restore a match. A replay of an event
ID with identical bytes is a no-op; the same ID with different bytes is invalid. Persist the HLC
and latest event with the profile so restarts do not reset the clock. Observe received timestamps
using the existing 60-second future-skew guard before applying them. A sender retries the same
event bytes and ID until recipient delivery is acknowledged; a retry must not allocate a fresh
HLC value. Processing and state sealing precede mailbox acknowledgement and notification.

Separate devices of one identity can issue concurrent events. The ordering above gives every
device the same result once they have the same events, but an offline device can supersede another
device's action. The UI must show the reconciled state after device sync; the store and delivery
tasks must replicate the event stream to the owner's other devices. A missing or evicted signal
can delay or prevent convergence until retry or later reconciliation; the sender cannot erase a
notification the peer already saw. The local own state changes immediately; remote match status
is provisional until delivery is acknowledged. No bootstrap node decides or stores a match.

The encrypted unlike was chosen over local-only undo because the peer otherwise keeps a match
that the sender believes was removed. It adds delivery and reconciliation work, and cannot retract
an earlier notification. Ordering the latest like/unlike event gives out-of-order delivery a
deterministic result; a causal reference to one like ID would handle that one event but would need
extra rules for repeated likes and concurrent devices. Keeping local pass history avoids sending
an unnecessary preference signal.

## Acceptance fixtures for the implementation

1. Encode/decode each v1 kind exactly; reject truncated, extra-byte, unknown-version and
   unknown-kind payloads; the short padded MLS envelope including a signed token stays at most
   1024 bytes.
2. Like A→B alone creates no match; like B→A creates one; unlike A→B removes it on both peers
   after delivery. Pass and undo-pass change only A's local pass history.
3. Apply unlike before its older like, then replay either event: state stays unliked. Apply a
   later fresh like: state becomes liked. Equal HLC values resolve by device ID then event ID.
4. Duplicate event ID with identical bytes is idempotent; conflicting bytes or a wrong group,
   sender, peer or future-skew timestamp leave state unchanged.
5. Two devices acting concurrently converge after receiving the same set of events. Restart
   preserves ordering and pending retries; offline delivery delays the peer's view of a match.
6. With view sending disabled, no view is queued; enabled sending queues one per peer per UTC
   day. A recipient can receive a view regardless of its own send setting.

The codec covers the binary round-trip and malformed-input part of fixture 1. The local state and
mailbox tasks must complete the size, state, replay, delivery and view fixtures.

`rust/client::store::Store::apply_interaction` (schema version 3) covers the local state part of
fixtures 2–5. It takes the sender and target identities, the sender device ID and a decoded
event, and in one transaction records the event in a replay ledger keyed by sender identity and
event ID and updates the latest like state of that direction. One of the two identities must be
the store's own. A view is recorded but never changes likes. `Store::likes` returns both
directions, and a match is both liked. The caller must still decode the payload and verify group,
sender and device before applying; the future-skew guard, the persisted own HLC and view-day
records are not implemented yet.
