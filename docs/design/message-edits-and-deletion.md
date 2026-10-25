# Message edits and deletion

This is the client contract for [best-effort deletion](../decisions/0011-best-effort-deletion.md).
No app exposes chat yet. The Rust chat codec carries a logical ID in tag 1 and parses tags 8 and
9; the client Store has a separate current-text projection keyed by logical message ID and can
apply edits/deletes to known originals. It still stores received inbox payloads by mailbox
envelope ID. Early-operation handling, Session wiring and UI remain planned. The recipient device
alone can sign a mailbox `Ack`.

## Identity and wire

Each text send gets a random 16-byte logical message ID before MLS encryption. The same ID goes
to every device in the conversation; the message key is `(group ID, author Peer ID, message ID)`.
The send path must persist and expose this ID even when a network put fails. The envelope ID is
not a message ID: a separate envelope is prepared for each recipient device.

The existing first-byte chat body tag remains the type boundary. Before the first release, tag 1
has changed from raw text to `message ID (16 bytes) || text`. Text and replacement text are
limited to 64 KiB by the codec. Tag 8 is an edit:
`target ID (16) || operation ID (16) || HLC (8, big endian) || replacement text`.
Tag 9 is a delete with the same 40-byte header and no text. An operation ID is random and stays
unchanged across retry. The sender identity and device key come from the verified MLS group,
never from an unauthenticated body field. Unknown tags, short IDs, trailing delete bytes and
oversized bodies fail closed. Text keeps the existing application validation and envelope size
limit; the codec must not silently truncate it.

## Applying operations

Only the author of the original text, in the same group, may edit or delete it. An operation
received before its original text waits under its claimed `(group, author, target ID)`; when the
text arrives, an author mismatch discards the pending operation. Distinct operations order by
`(HLC, sender device key, operation ID)`, as private interactions do. The existing 60-second
future-clock guard applies before state changes. An identical operation ID, body and sender
device is a no-op; the same ID with different bytes or device is invalid. A later edit replaces
the current text and sets an `edited` flag. Delete is terminal: it removes current text and leaves
a placeholder. Older text and later edits cannot restore it. Two devices converge after receiving
the same operations, subject to the existing device-sync work.

The local store retains one current text or deletion placeholder, a digest of the original text,
the ordering tuple, and a digest of each applied operation for replay conflict checks. It does
not retain prior revisions as app history. Once a received text is applied, its original inbox
payload must no longer be an alternate plaintext history. Physical remnants in SQLite WAL,
backups or screenshots cannot be reliably erased. The UI must not promise that.

Text and operation state, sealed MLS state and exact outbound envelopes must be staged durably
before the first network put. Incoming operations and pending mailbox ACKs must be committed
before acknowledging receipt. A retry or restart reuses the same operation ID and encrypted
envelopes. A delete shown locally before delivery is pending on the peer until its receipt.

## Best-effort node removal

The sender sends the delete as an encrypted MLS operation. On receipt, the recipient replaces
the text and sends its existing device-signed `Ack` for any known original mailbox envelopes.
Normal receive already ACKs envelopes after durable processing, so a delivered original is
usually gone from cooperating nodes. If the original arrives after the delete operation, the
recipient keeps the placeholder, discards the text and ACKs that envelope. A recipient that is
offline cannot make nodes remove an unread original; node TTL and operator behavior still apply.
This is a request through the recipient, not a claim of remote erasure.

| Mechanism | Pros | Cons | Decision |
| --- | --- | --- | --- |
| Encrypted delete, then recipient-signed ACK | Reuses the authenticated mailbox protocol; nodes learn no author identity or operation kind. | Removal waits for recipient delivery; uncooperative devices or nodes may retain copies. | Use for the first release. |
| One-time sender removal capability per envelope | Can request removal while the recipient is offline without revealing profile identity. | Adds envelope metadata, locally retained secrets, replica tombstones and a new abuse surface. | Reconsider only if offline pre-delivery removal becomes a requirement. |
| Profile-signed sender tombstone | Straightforward owner signature. | Links a private message to the public profile at the node, defeating sealed sender. | Reject. |

Verification needs codec malformed-input tests; local author, ordering, replay, restart and
no-history tests; and two-client tests for edit, delete, out-of-order delivery, offline retry and
recipient ACK. UI tests should confirm the edited mark, deletion placeholder and honest warning.
