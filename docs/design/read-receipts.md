# Read receipts and typing indicators

This is the planned client contract for read receipts and typing indicators under
[ADR 0012](../decisions/0012-private-p2p-interactions.md): both are MLS messages between chat
members, never stored on nodes beyond mailbox delivery, and each can be turned off locally.
Implemented in the Rust client: the read receipt wire body (`Body::Read` in
`rust/client/src/chat.rs`, sent with the signal class and never shown in history), the
setting and read marks in the store (`rust/client/src/store/reads.rs`), and sending with
`Session::mark_read` and applying in `Session::receive`. Planned: the app UI, and typing
indicators.

Read receipts are distinct from delivery receipts (`Body::Receipt`, tag 5,
[messaging](../architecture/messaging.md)): a delivery receipt says a device committed envelopes
from its mailbox and drives the sender's outbox; a read receipt says the user saw messages and
only changes what the sender's app shows.

## Read receipts

### What a peer learns

When the setting is on, a chat member sees **read** on their own messages up to the latest one
the user has seen, and roughly when the user read it: a receipt is sent when the app shows the
messages, so its arrival tracks reading to within the recipient's mailbox polling. There is no
read time on the wire.

### Setting

`send_read_receipts` is a local setting of the identity, never published:

| Setting | Effect |
| --- | --- |
| Off (default) | Send nothing. |
| On | Send a read receipt to every unblocked member of a one-to-one chat. |

Off is the default for the same reason as profile views: a read mark tells the sender when the
user opens the chat, and the user should choose that. As with views and presence there is no
reciprocity: turning sending off does not stop receiving and showing a peer's receipts.

### Wire

A read receipt is a chat body with tag `11`: the 16-byte logical ID of the latest text the
reader has seen in that chat, nothing else (17 bytes). The sender and device come from the
verified MLS group. It is short-padded to 256 bytes and put with `class = signal`, so a node
cannot tell it from a like, a view or presence. A wrong length fails closed and the envelope is
acknowledged without changing state.

Naming one ID, not a list, keeps the body fixed-size: the author orders their own messages, so
"read up to this one" marks it and every earlier own message in that chat. Texts are the only
bodies with a logical ID today; an attachment counts as read when a later text is read, and gets
its own mark once attachments carry an ID.

### Sending

The app calls the client when it shows a chat's messages. The client sends one receipt to the
chat's other members when the setting is on, the peer is not blocked, and the latest seen text
from that peer is later than the one last receipted in that chat. Scrolling back sends nothing,
and reopening a chat with no new text sends nothing. "Later" is the order in which a device
recorded the texts; the author's own texts are recorded in the order they were sent. A receipt is
retried like any outbox write, gets no delivery receipt itself and has no expiry: a late read
mark is still true.

### Receiving

The receiver drops a read receipt from a blocked peer and from a sender that is not a member of
the group. An ID that is not one of the receiver's own texts in that group is ignored, as is one
that is not later in the receiver's order than the read mark it already holds. The read mark
never notifies and never changes the messages.

### Limits

- Only the reading device sends; the user's other devices do not learn what was read until
  device sync exists.
- Group chats are out of scope: only one-to-one groups are accepted now.

### What a node learns

The node holding the author's mailbox sees a signal arrive and the reader's IP address, as for any
signal. A signal sent right after a fetch can link the reader's mailbox to the author's; this is
the residual in [traffic analysis](../security/threat-model.md#traffic-analysis), the same one
as for presence. Sealed sender keeps the reader's identity out of the envelope.

## Typing indicators

Typing indicators are not sent through mailboxes. Delivery is store-and-forward and the recipient
fetches on its own schedule, so a typing signal would usually arrive after the message it
announces. Each burst would also take a signal envelope from the recipient's signal quota, which
evicts the oldest signals first and could push out a like.

Planned: typing indicators over a direct client-to-client path, when one exists, as an MLS message
that is never stored: tag `12`, sent at most once every 5 seconds while the user types, shown for
10 seconds after the last one, and turned off by a local setting `send_typing`.

## Alternatives not taken

- **A list of read IDs:** names attachments too, but the body grows with the list and needs a
  cap to fit the signal limit; "read up to" says the same for one-to-one chat.
- **Read time on the wire:** more precise than arrival time and a finer usage log.
- **Reciprocal setting** (turning receipts off hides the peer's): common in messengers, but views
  and presence have no reciprocity, and one rule is easier to explain.
- **Typing through mailboxes with a short expiry:** bounded cost, but still late and spends the
  signal quota.
