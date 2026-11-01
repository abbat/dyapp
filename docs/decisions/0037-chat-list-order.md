# 0037. Chat messages carry an HLC; unread is local state

- **Status:** Accepted
- **Date:** 2026-11-01

## Context

ADR 0010 orders messages by hybrid logical clock (HLC), with Peer ID as a tie break.
The current MLS chat text body carries only a random message ID and text. The client
stores its current text, edits and deletion marker, but no original timestamp or
per-message unread state. A Chats list cannot show a truthful last-message time,
stable order or unread count from these rows.

## Decision

- New text messages use chat body tag 12: the tag byte, 16-byte logical ID,
  eight-byte big-endian packed HLC, then up to 64 KiB of text. Tag 1 remains
  readable during migration. A sender advances its durable HLC before staging
  the MLS body; a receiver rejects a remote clock more than 60 seconds ahead,
  as in ADR 0010. Tag 12 must not be sent until every supported app can decode
  it: an older client may acknowledge and discard an unknown tag. The rollout
  therefore deploys decoders first and enables tag 12 sending in a later step.
- The client store keeps the original HLC on each current message. A conversation
  combines all MLS groups for that pair. Message order is original HLC, author Peer
  ID, logical ID, then group ID as a final tie break. The conversation row uses its
  latest current message (including a deletion placeholder) and that message's
  original time; an empty row has no last-message time. Editing and deleting
  change content or marker in place, not the order. Rows from old bodies have no
  original HLC, appear before timestamped rows in local arrival order, and show no
  invented time; their cross-device order is unknowable.
- The store keeps a local unread flag on each incoming text. A received text starts
  unread; opening that chat marks its current messages read in one transaction.
  Own messages are read. An edit or delete does not create another unread message.
  Clear chat deletes the current rows in each group of the pair and keeps the
  existing key-only cleared marks against replay, so unread becomes zero while
  the conversation state and empty chat row stay. Read state is per device and
  is not sent to peers.
- A new store schema migration adds nullable original HLC and an unread flag to
  current messages. Old rows start read and without a timestamp; neither value is
  fabricated from a SQLite row ID or a later edit's HLC.

## Alternatives and trade-offs

| Approach | Pros | Cons |
|---|---|---|
| New text tag and original HLC (chosen) | Satisfies ADR 0010 and keeps old text readable; time and order travel with the authenticated MLS body. | Wire and storage migration; rollout must update all supported clients before tag 12 is sent, or old clients may acknowledge and discard it. |
| Local receive time only | No wire change. | Offline delivery or another device gives the wrong sent time and a different order. |
| SQLite row ID as message time/order | No migration. | It is neither a time nor a stable cross-device order and may be reused after deletion. |
| Per-chat last-seen HLC for unread | Small state. | A late offline message with an older HLC would silently count as already read. |

## Consequences and verification

Core tests cover new and legacy text decoding, clock bounds, stable order across
edit/delete and restart, late delivery as unread, marking a chat read, Clear chat,
and profile isolation. Desktop tests cover last-message time and unread count.
This ADR updates the chat storage and wire contract; it does not add peer-visible
read receipts.
