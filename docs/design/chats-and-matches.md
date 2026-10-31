# Likes, matches, message requests and chats

This is the planned per-pair state machine for [ADR 0012](../decisions/0012-private-p2p-interactions.md).
It combines the existing like/unlike ledger ([private interactions](private-interactions.md)),
MLS chats ([messaging](../architecture/messaging.md)) and the desktop screens
([Found profile and Chats](desktop-screens.md#chats)). Likes, matches, texts, the
store's conversation state (`Store::conversation`), the request's text-only rule and the
canonical pair group (`Session::send_text_to`) work in the Rust client today, and the FFI exports
them as `ChatSession`; the app UI below does not exist yet.

## State of a pair

Each local identity keeps, per peer identity, four independent parts. None is published or kept by
a node.

| Part | Values | Source |
| --- | --- | --- |
| Own like | none, liked, unliked | Latest own like/unlike event |
| Peer like | none, liked, unliked | Latest received like/unlike event |
| Conversation | none, request sent, request received, open | Pair group texts and matches |
| Block | off, on | Local only (`Store::set_blocked`) |

A **match** is derived, never stored: both likes are liked. The screens show the pair as:

| Shown as | Condition |
| --- | --- |
| Liked you | Peer liked, own not liked, no block |
| Match | Both liked, no block |
| Message request (`chat-request`) | Conversation is request received, no block |
| First message sent (`chat-first`) | Conversation is request sent |
| Chat (`chat`) | Conversation is open, no block |

## Transitions

| Event | Conversation before | After |
| --- | --- | --- |
| Own text to a non-match | none | request sent |
| Own text | request sent | refused locally: one pending message |
| Peer text | none | request received |
| Peer text | request sent | open (the reply is consent) |
| Own text (Reply) | request received | open |
| Further peer text | request received | acked and dropped, no history |
| Match applied (either side's like completes it) | any | open |
| Unlike, either side | open | open: the match ends, the chat stays |
| Clear chat | any | unchanged; history only is removed |
| Block | any | unchanged, hidden; peer traffic acked without history |

**Open is final.** It is set once, in the same transaction that records the text or like that
causes it, and is never cleared: not by unlike, Clear chat or unblock. Only removing the identity
deletes it. After an unlike the two keep writing; Block is the way to stop a peer.

**No daily limit.** A count of first messages per day would only be checked by the sender's
client, which anyone can change, so there is none. The one-pending rule holds because the
*recipient* enforces it: while a request is unanswered, a second text from that peer with another
logical ID is acked and dropped. Edits and deletes of the pending text still apply. Node limits
(signal quota, tokenless mailbox pool, puzzles) and local Block bound the rest.

A request is text only (tag 1). Before the conversation is open the recipient acks and drops a
non-match's attachments, forwards, presence, read receipts and token requests, and answers no
token request: a stranger gets no write token until the recipient replies or a match exists.
Likes and views follow their own rules and are accepted either way.

## One group per pair

A chat is the MLS group of exactly the two identities' devices. Both peers, or two devices of one
peer, can create one at the same time, so a pair may end up with several groups. Each side then
keeps every such group's MLS state and shows their texts as one chat ordered by HLC. It sends new
messages only to the **canonical group**: the one with the lowest group ID among those it holds
for the pair. Both sides reach the same choice once each has every Welcome. A message already
sent to another group is still shown, so nothing is lost. A group that also contains a third
identity is refused, as today.

The first own text to a peer with no group runs `chat::invite` with the peer's verified
KeyPackages, then stages the text in the same epoch; the Welcome and the text share one outbox
commit. Retries reuse the staged bytes, as for every chat write.

## Idempotency and replay

- Texts, edits and deletes keep their logical and operation IDs; a replay is a no-op.
- Like, unlike and view events keep the ledger rules of the interaction design.
- A replayed Welcome for a group already held is ignored.
- Open is set-only, so applying the same events in any order gives the same result.
- A request is counted per pair across all its groups, so a second group cannot carry a second
  pending text.

## Own devices

All devices of both identities are members of the pair group, so every own device sees the same
texts and derives the same conversation state. Likes reach own devices only with the planned
device sync of the interaction design. The cases:

1. Two own devices send a first text at once: each makes a group, the canonical rule merges them;
   the recipient keeps the first text it receives and drops the other, which stays Sent, never
   Delivered, on the second device.
2. One own device sends the request and another replies to a peer text: both see the group, so
   both show the chat as open.
3. A like from one own device and an unlike from another: the ledger order decides; the chat stays
   open if it was.
4. A device offline during a match: it shows Liked you until device sync brings the own like.
5. Block on one device: local until device sync; the other device still shows the chat.

## Tests for the implementation

1. Store: each transition above, including the refused second request, open surviving unlike,
   Clear chat and unblock, and the transaction that sets open with its cause.
2. Receive: a second request text is acked without history; an edit of the pending text applies;
   a non-match's attachment and token request are dropped before a reply and handled after it.
3. Two clients: request, reply, open; like, like back, open; unlike keeps writing possible; block
   stops history.
4. Concurrent creation: both peers send a first text at once, both show one chat with both texts
   and send the next text to the same canonical group.
5. Restart: a staged Welcome and first text retry with the same bytes; open survives a restart.
6. Apps: the Found profile, Liked you, Message requests, `chat-first` and `chat-request` states
   follow the table in "State of a pair".
