# Likes, matches, message requests and chats

This is the planned per-pair state machine for [ADR 0012](../decisions/0012-private-p2p-interactions.md).
It combines the existing like/unlike ledger ([private interactions](private-interactions.md)),
MLS chats ([messaging](../architecture/messaging.md)) and the desktop screens
([Found profile and Chats](desktop-screens.md#chats)). Likes, matches, texts, the
store's conversation state (`Store::conversation`), the request's text-only rule and the
canonical pair group (`Session::send_text_to`) work in the Rust client today, and the FFI exports
them as `ChatSession`, whose `profiles` call fetches the verified profiles behind the Liked you
list. The desktop shows Like and Liked in the found profile, the unlike
confirmation and the Liked you list; Android lists the chats, receives the mailbox, sends text
from the chat screen and shows Like, Liked and the unlike confirmation in the found profile; the
rest of the app UI below does not exist yet.

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

The Linux Found profile's Message action opens a `none` conversation to send one first text.
The Chats composer then follows these states: `requestSent` disables another send,
`requestReceived` allows one reply, and `open` allows later text. It keeps a draft when a send
fails before local staging. If a network failure occurs after staging, it shows the local text
and pending delivery; the desktop minute mailbox poll retries due outbox writes. Block from
the Linux chat detail hides the peer immediately on this computer; network token revocation may
remain pending and can be retried in Settings.

**No daily limit.** A count of first messages per day would only be checked by the sender's
client, which anyone can change, so there is none. The one-pending rule holds because the
*recipient* enforces it: while a request is unanswered, a second text from that peer with another
logical ID is acked and dropped. Edits and deletes of the pending text still apply. Node limits
(signal quota, tokenless mailbox pool, puzzles) and local Block bound the rest.

A request is text only. The shared decoder accepts legacy tag 1 and timed tag 12.
As of 2026-11-01, `gh release list --limit 20` found no published releases, and no
client app has shipped. The decoder-first condition in
[ADR 0037](../decisions/0037-chat-list-order.md) is therefore satisfied before the first
release; `Session::stage_text` and `Session::stage_text_to` now send tag 12. They draft one
HLC before encryption and commit it with MLS state, the outbox and the local text.
Recheck the supported release inventory before any later wire change.

Before the conversation is open, the recipient acks and drops a non-match's attachments,
forwards, presence, read receipts and token requests, and answers no token request: a stranger
gets no write token until the recipient
replies or a match exists.
Likes and views follow their own rules and are accepted either way.

## One group per pair

A chat is the MLS group of exactly the two identities' devices. Both peers, or two devices of one
peer, can create one at the same time, so a pair may end up with several groups. Each side then
keeps every such group's MLS state and shows their texts as one chat ordered by HLC. It sends new
messages only to the **canonical group**: the one with the lowest group ID among those it holds
for the pair. Both sides reach the same choice once each has every Welcome. A message already
sent to another group is still shown, so nothing is lost. A group that also contains a third
identity is refused, as today. Timed texts use their original HLC for ordering. Legacy tag-1 texts
have no sent time and use a non-reused local arrival ordinal as a fallback before timed texts.
`Store::current_chat_messages` exposes that order across every group of one pair, including the
current edit or deletion marker. `Store::chat_rows` supplies pair state, the latest current
message and its optional original time, and the device-local unread count; empty conversations
remain after Clear chat. The desktop now exposes read-only `chat_rows` and `chat_history` Tauri
commands for the active identity. `chat_rows` returns `peerId`, `state`, `lastMessage` and
`unreadCount`; `chat_history(peerId)` returns `state` and ordered `messages`. A message includes
opaque group and logical IDs, current text or a deletion marker, local unread and optional
`sentAtMs`. The commands hide blocked peers, return original wall time in JavaScript-safe
milliseconds, and leave legacy text time absent. `send_chat_text(peerId, text)` now refreshes
the signed peer profile on the active network and uses `Session::send_text_to` under the identity
lease and MLS-session mutex; a failed network send retains the staged outbox bytes. It rejects an
unpublished own profile or device, a blocked or self peer, stale profile data and invalid text.
`clear_chat(peerId)` clears all local groups for that pair in one transaction and returns the
number of removed current messages; the conversation state and empty row stay. It rejects an
unknown, blocked or self peer. `mark_chat_read(peerId)` clears the current unread flags on
this device across the pair's groups when the chat is opened; it sends no peer-visible receipt.
The Chats UI call site remains planned.

The Store marks each accepted incoming text unread on this device; own texts are read. Edits and
deletes do not add unread items. Opening a chat will call `Store::mark_chat_read` for the peer,
clearing the current unread rows across all pair groups atomically. A later delivery starts unread
even if its original HLC is older than a message already viewed. This local flag does not depend
on the optional peer-visible read-receipt setting. `Store::clear_chat_pair` clears all cataloged
groups for one peer in one transaction, keeping key-only replay marks and the conversation state.
The Chats UI still needs to call these APIs.

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
