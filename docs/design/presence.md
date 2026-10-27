# Online presence

This is the planned client contract for presence under
[ADR 0012](../decisions/0012-private-p2p-interactions.md): a match can see that the other side
used the app recently, and the user can hide it. Only the wire body is implemented
(`Body::Presence` in `rust/client/src/chat.rs`, sent with the signal class and dropped on
receive); the audience setting, sending, receiving state and app UI are planned.

## What a peer learns

Presence is coarse. The app shows **active recently** for a match whose latest presence signal
is at most 2 hours old by the receiver's clock, and nothing otherwise. There is no "online now",
no "last seen" time and no "offline" signal. A peer who keeps the signals can still log their
times, so presence tells an allowed match roughly when the user opens the app, to within the
send throttle below. That is the trade-off the user accepts by leaving presence on.

## Audience

The audience is a local setting of the identity, never published:

| Setting | Effect |
| --- | --- |
| Matches (default) | Send presence to every current match. |
| Nobody | Send nothing. Marks shown by peers expire within 2 hours. |

A match exists only while both latest like states are liked
([private interactions](private-interactions.md#state-and-ordering)), so an unlike or a block stops
presence on the next send. A per-match choice is later work. As with views, there is no
reciprocity: hiding one's own presence does not stop receiving a match's presence.

## Wire

Presence is a chat body with tag `10`: `HLC (8 bytes, big endian)`, nothing else. The sender and
device come from the verified MLS group, like every chat body. It travels as an MLS application
message to each device of the match, short-padded to 256 bytes, in an envelope with
`class = signal`, so a node cannot tell it from a like or a view. A wrong length fails closed and
the envelope is acknowledged without changing state.

## Sending

When the app comes to the foreground, the client sends one presence to each allowed match it has
not sent one to in the last hour. It never sends from the background or on a timer, so presence
follows real use, not a heartbeat. The outbox drops an unsent presence older than 2 hours instead
of retrying it: a late presence is wrong, not merely slow.

The one-hour throttle bounds the cost: a recipient offline for a day with 50 active matches
receives at most 1 200 presence envelopes of about 640 bytes, under half of the default
2 MiB signal quota. The quota evicts the oldest signals first, so a long-offline recipient with
many matches can still lose an old like to presence; the throttle keeps that rare.

## Receiving

The receiver applies the 60-second future-skew guard to the HLC, then keeps only the latest
presence per peer identity. It drops presence from a blocked peer and from a peer that is not a
current match, before observing its HLC. A presence never notifies and never changes likes,
views or chat.

## What a node learns

The node holding the recipient's mailbox sees a signal arrive at that mailbox and the sender's IP
address, as for any signal. Bursts of signals to several mailboxes from one address when the app
opens can link those mailboxes to one sender; this is the residual in
[traffic analysis](../security/threat-model.md#traffic-analysis). Sealed sender keeps the
sender's identity out of the envelope.

## Alternatives not taken

- **Direct peer-to-peer liveness:** precise and leaves nothing on nodes, but needs reachability
  between clients that does not exist yet, and reveals each side's IP address to the other.
- **"Last seen" time:** more useful, but turns every match into a usage log at full precision.
- **No presence:** no leak at all; ADR 0012 keeps presence as a user choice instead.
