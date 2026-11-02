# Messaging Architecture (Offline-First)

Authored Rust APIs document local status changes and plaintext retention. Attaching encrypted
bytes to a `Message` does not by itself encrypt, verify receipts or send it.

## Overview

Message edits and deletion follow a [staged design](../design/message-edits-and-deletion.md).
The chat codec carries a logical text ID and parses edit/delete operations. `Session::stage_text`
returns a new ID with its exact queued writes before network delivery; `send_message` is the
convenience path that immediately attempts those writes. `stage_edit` and `stage_delete` use
the same path for an own, present text; `send_edit` and `send_delete` attempt delivery after
staging. A separate local projection persists initial text by that ID and applies edits/deletes,
including operations that arrive before the original within a 64-operation pending limit. `Session` now commits received
text, edits and deletes with sealed MLS, inbox dedupe and ACK intent in one transaction. UI remains
planned. The recipient device alone can sign its mailbox `Ack`, so deletion cannot be
presented as guaranteed remote erasure.

Message model and hybrid logical clock in `rust/messaging`. The only delivery path is the MLS
`Session` with its SQLite outbox in `rust/client`; the earlier in-memory plaintext queue was
removed so it cannot become a second, unencrypted one.

Delivery of MLS chats lives in `rust/client` (`dyapp-client`): `Node` speaks the profile,
KeyPackage and mailbox protocols to one bootstrap node, and `chat` sends every Welcome, commit
and padded message as MLS bytes, one envelope per member device. Calls that change MLS state
return the deliveries instead of sending them, and `chat::receive` returns the envelope ids to
ack, so the client seals the state first. The app does not use it yet. See
[Encryption & Security Status](../security/encryption.md).
`prepare_put` can sign a mailbox write before network activity and return exact bytes, mailbox
and ID. `PreparedEnvelope::from_bytes` verifies a stored copy; `Node::put_prepared` reuses that
record on retry. `Node::put` still prepares and sends in one call for existing callers.
`prepare_tokened_put` carries a held
WriteToken and signs with its token key instead of a one-off key. `tokens::send` uses today's
held token, resends tokenless on DENIED, and on STALE tries tomorrow's token, then tokenless.
`Session::send_message` stages writes with today's held token the same way; a refused one is
swapped for its replacement in the outbox in one commit before it is put again.
`Session::receive` asks the group for a month of tokens after a join, answers a contact's
`TokenRequest` and asks back, and stores a `TokenAnswer`; these bodies are kept as state changes,
not history, and are not returned to the app. When an incoming reply or match opens a chat,
`Session::receive` asks again because the join request may have been dropped while pending.
The desktop catch-up pass calls `Session::refill` when its persisted due time arrives, at most
once per 24 hours after success. A failed attempt stays due for the next pass; another request
reuses the saved token keys, recovering a lost earlier request. Other app adapters still need
this schedule. Nodes
default to `limits.require_write_tokens`, so a contact's tokened write passes a mailbox whose
tokenless pool a stranger has filled.
`tokens::revoke` marks every key a device issued to a contact revoked and sends its node a
device-signed `revoke`, 64 keys at a time, so that contact's later writes fall back to the pool.
`Session::block` records a local block (store `peers` table) and signs the contact no more
tokens. If no live token keys were issued, it needs no node. Otherwise it sends a revoke through
the first node by score; a failed revoke keeps the block and can be repeated. The desktop
`block_peer` command accepts a well-formed Peer ID even if it was not in the current search
results, returns `notNeeded`, `pending`, or `confirmed` for this attempt, and never rolls back
the local block after a network failure. `confirmed` means one node accepted, not that every
replica did. The UI still needs a visible Retry control for pending or unknown network status.
`Session::unblock` clears the flag only: revoked keys stay revoked, and the contact gets tokens
again for the days it asks anew.
On receive, a blocked owner's Welcome is refused, and its messages advance MLS state and are
acknowledged but get no history, receipt or token answer; its likes and views are dropped too.
`search::search` takes `Store::blocked_peers` and neither fetches nor returns their profiles.
Like signals use shorter plaintext padding than ordinary chat. Message requests to non-matches
follow the planned [per-pair state machine](../design/chats-and-matches.md).
The measured envelope sizes and 1 KiB limit are documented under
[bootstrap mailbox envelopes](bootstrap.md#mailbox-envelope).
The like, unlike and view payload and match state are specified in the
[private interaction design](../design/private-interactions.md).
`rust/client::interactions` now encodes and strictly decodes the 26-byte v1 plaintext for those
three kinds. `chat::Body` prefixes every chat plaintext with a kind tag (`1` text, `2`
interaction, `3` write-token request, `4` write-token answer, `5` delivery receipt, `6`
attachment, `7` forwarded text or attachment, `8` edit, `9` delete, `10` presence, `11` read
receipt, `12` timed text), so a text is never read as
an interaction; an unknown tag is refused and acked. Legacy tag 1 contains a 16-byte
logical ID before at most 64 KiB of text. Tag 12 adds an eight-byte big-endian HLC
after the ID, with the same text limit. No client release existed on 2026-11-01,
so tag-12 sending may start before v0.0.1 while tag-1 decoding remains.
Tags 8 and 9 contain a 16-byte target ID, 16-byte operation ID and
big-endian 8-byte HLC; edit adds at most 64 KiB of replacement text, while delete has no tail.
Tag 6 now carries a required 16-byte message-scoped logical ID and sender HLC alongside the
media descriptor. This lets repeated sends of one blob remain distinct and supplies an ordered
history key; the attachment projection in app Chats is planned. The codec refuses short IDs,
missing or out-of-range attachment HLCs, oversized text, trailing delete bytes and forwarded
operations.
The desktop `send_chat_text` command fetches a verified peer profile before sending, checks it
against the newest profile held in the active network cache, and serializes the MLS Session with
other desktop sends and mailbox receives. A delivery failure leaves the exact staged bytes in
the outbox for retry.
A forward wraps the tagged text or attachment and names no original author. Received forwarded
text is stored with a durable marker and the MLS sender as author; Linux Chats shows the label.
Outbound Session and the Linux Forward picker stage a forwarded text with exact retryable writes
in an existing open chat. An attachment body carries the `Attachment` descriptor:
the key, plaintext and blob SHA-256 and release secret, 32 bytes each, the plaintext size, an
optional JPEG preview of at most 24 KiB, a printable-ASCII MIME type of at most 64 bytes, and
the claimed width and height; any other length is refused. A message carries at most 64 KiB of
previews: each of `n` images gets
`attachment::preview_budget(n)`, min(24 KiB, 64 KiB / n). `Session::send_attachment` uploads the
sealed blob to the first node that takes it before it stages the message. One attachment is one
blob, so its sealed size is capped by the node media limit of 6 MiB: there is no split into
several blobs. `Session::send_attachment_to` sends into the pair's joined group of an open
conversation only. The Linux composer re-encodes a picked JPEG or PNG on a canvas to a JPEG of at
most 2560 px and 6,000,000 bytes, stepping down to 1280 px, with a preview of at most 24 KiB, and
`send_chat_photo` seals and sends them. Size handling on the other apps is planned.
`attachment::fetch`
reads and opens it on receipt. Recipients never release the blob: every device of the chat is an
MLS member and the first release would drop it for the others, so it expires after the node's
retention. `attachment::auto_download` downloads a contact's original without a tap only on an
unmetered network (or in the "always" mode) and a stranger's only on a tap.
The desktop `chat_photo_original` command fetches one original and checks its real dimensions from
the header before the page decodes it, at most 64 megapixels. On each mail pass of an open chat the
page asks `chat_photo_auto` whether waiting originals may download: Linux reads NetworkManager's
`Metered` property over the system bus, and any other platform or an unavailable service counts as
metered. The automatic path covers only the open chat. An original this device opened is sealed
into `originals/` of the identity's folder up to the limit in Settings (`store::originals`), and the
command answers from there without a node, so only a photo not opened within the retention loses its
original.
`attachment::hours_left` gives the "N more hours" label for the last day of the 168 h retention. A request holds 1 to 1,920 asks (30 days for
64 owner devices), each a 32-byte mailbox, a UTC day and a 32-byte token key; an answer holds 1
to 1,920 non-empty signed `WriteToken` records, which the receiver still verifies.
`tokens::answer` signs, with the device mailbox key, only asks for its own mailbox and a day from
yesterday to 30 days ahead, records each key in `issued_tokens`, skips a revoked key or one issued
to another contact, and splits the answer so each padded body is at most 64 KiB.
`tokens::request` asks, for each owner device mailbox holding fewer than 15 days, for every
missing day of the next 30 with a sealed token key generated on first use, so a retry reuses it;
`tokens::accept` stores a token only if its signer's mailbox is the token's mailbox and its key
is the one held for that mailbox and day. A Rust test
sends an interaction through the existing MLS chat path and decodes the result.
`Store::apply_interaction` keeps a replay ledger and the latest like state per direction, and
`Store::likes` derives a match from them. `Store::interact` creates own events from a persisted
HLC and creates at most one view per peer per UTC day; `Store::receive_interaction` applies the
60-second future-skew guard first. `chat::message` pads an interaction body from 256 bytes, not
1 KiB, and `Session` puts it with `class = signal`, keeping the class on a token fallback; a
signal whose signed payload is over 1 KiB is refused before staging, never truncated.
`chat::apply` returns a received interaction as `Event::Interaction` with the sending device's
mailbox key from its certificate (`mls::Received::Message::device`), which orders concurrent
events of one identity. `Session::interact` creates an own like, unlike or view for the other
owner of a group, commits the event with sealed MLS state and exact outbox bytes, then sends it,
so a failed put retries the same event even after restart;
`Session::receive` applies a received one to the ledger before acking it, refuses one from the
future and applies one from another own device as its own, against the other owner of the group.
`Session::likes` reads the
match state. A protocol test covers a match, a replayed put, an offline unlike, one view a day and the
signal class.
The Linux desktop Search and Liked you views show likes; other platform UI remains planned.
`rust/client::store::Store::open` now creates or reopens an identity-bound, version-1 SQLite
database at an absolute path supplied by the app. It refuses a different identity, unknown
unversioned objects, corrupt data and future schema versions. WAL uses full synchronous commits.
The store persists the view-sending choice and local pass/undo state with a privacy-preserving
default of no view signals. See the [client storage design](../design/client-local-storage.md).
The store can now commit a caller-provided sealed MLS snapshot and exact prepared mailbox writes
in one SQLite transaction, reopen due writes after restart and schedule a later retry. A conflict
on an existing mailbox/ID with different signed bytes rolls the whole transaction back. The
group catalog records group IDs and verified peer associations with the sealed Device and
prepared Welcome writes; `load_groups` restores those groups after restart and refuses missing
or mismatched entries.
`rust/client::session::Session` opens the sealed Device and cataloged Groups. Its send path
encrypts one chat message, prepares every signed mailbox write, commits the new seal and all
writes, and only then calls `Node::put_prepared`. A preparation or local storage failure discards
the advanced in-memory handles until `recover` reopens the last committed snapshot. Network
failure leaves the exact signed writes in the outbox: `Session::retry_due` puts every due one
again, and a failed retry waits 1 s, then twice as long each time, up to an hour. An accepted
write stays until its delivery receipt: `Session::receive` sends each group one `Body::Receipt`
naming the envelope IDs it committed, and the sender's outbox drops them. Without a receipt it
is put again 1 h after acceptance, then up to a day apart, and given up after 30 days. A receipt
leaves the outbox once a node accepts it and is not receipted itself. Each `Session` call takes
the caller's nodes, e.g. one per replica key: a put tries them lowest penalty first
([scores](bootstrap.md#replication-and-repair)), the node that accepted the write last tried
last, and each failure counts against its node; `receive` fetches from the first that answers. A
receipt for a later write to the same mailbox counts against the node that accepted a
still-waiting earlier one, which is resent at once. The Linux desktop's minute catch-up pass
calls `Session::retry_due` before receiving mail, under the active identity lease and the shared
Sessions lock; due writes are retried after reconnect or restart without drafting new text.
For a staged own text, edit or delete, the outbox now records the logical message ID and the
current content generation (the original ID or edit/delete operation ID) alongside its MLS group.
The current chat projection records that generation as pending in the same transaction; incoming
operations clear a superseded local generation. Node acceptance atomically advances the current
local generation to node-accepted; a signed recipient-device receipt advances it to received by
at least one device. A late receipt for an older text or edit cannot change the newer generation.
Those states survive restart and outbox removal. Chat writes have no expiry before acceptance;
an accepted write given up after 30 days still has the factual node-accepted status. Linux
Chats reads these persisted states for its own-message labels after reopening and refreshes an
open chat after the minute catch-up pass, including a receipt with no new visible text.
The desktop app also uses this adapter for likes, token refill, and own-message edit/delete
commands. Edit/delete validate peer, group and logical message ID against the local projection
before opening a Session. Linux Chats exposes Edit and Delete for own live text; other
platform messaging screens remain planned.
It can also commit an incoming envelope ID, caller-provided application payload and new sealed MLS
snapshot in one transaction. The caller checks the ID before applying MLS; after commit it can
acknowledge pending IDs, and a restart reopens the ACK queue. Duplicate IDs never overwrite the
saved MLS snapshot. `Session::receive` wires this sequence to a fetched mailbox: it skips known
IDs, applies one envelope at a time, commits it (a Welcome also records its group and peer in the
catalog; text, edit and delete update the current projection and leave only a body digest in the
inbox; other visible bodies still use protobuf `StoredMessage`) and only then returns the event
and ACKs every pending ID of that mailbox. An original text already received is therefore ACKed
before a later delete; a repeated original is ACKed again without restoring deleted text. If an
authenticated delete is staged before its original at the Store boundary, that original keeps the
deleted placeholder and gains its own ACK intent. An offline recipient cannot ACK unread ciphertext.
Gap and unknown-owner envelopes stay unrecorded and unacked; an envelope from a device the held
profile does not list counts as unknown-owner, since that profile may predate a link, and waits for
a newer profile or for the node to expire it. A refused envelope stores only a marker,
keeps the previous MLS snapshot and reopens it. Only one-to-one groups are accepted for now, and the replica cross-check does not use this path yet.
Node tests run two such sessions against a mailbox node: one covers restarts and injected storage
faults at both boundaries, and another stages edit and terminal delete while offline, reopens the
sender's store, retries the original encrypted writes, and checks both local projections, ACKs and
digest-only receiver inbox records. See the
[acceptance matrix](../testing/bootstrap-acceptance.md).
The [session recovery contract](../design/client-local-storage.md#session-recovery-contract)
specifies that a storage error invalidates the current in-memory MLS session, and that receive
must process one envelope per transaction before UI display or ACK; `Session::receive` is that
adapter, and the desktop catch-up pass calls its paged form.

### Catch-up checkpoints

The desktop [catch-up runtime](../design/client-local-storage.md#catch-up-runtime) owns due
outbox sends, mailbox fetches and ACK retries for the active slot: a runtime thread runs a
pass every minute, and Chats requests one when it opens. Mobile runtimes are planned.

Mailbox fetch can now return a cursor after at most 100 records ordered by class and
insertion sequence. `Node::fetch_page` signs a fresh challenge with an explicit
1–100 record limit and optional node-local cursor; it rejects a missing or unchanged
next cursor when a node reports more. Legacy `Node::fetch` still returns its first
page and `more` without requiring cursor support. `Session::receive_page` now
processes at most 20 records through the existing per-envelope commit, ACK and
receipt path, then saves the node-bound cursor or clears it at the end. Its
caller must schedule another pass when `more` is true. The desktop pass does
so, so an unacknowledgeable Gap or unknown-owner record in a full first page
no longer hides later records; see
[client pagination](../design/client-local-storage.md#mailbox-pagination).
Cross-replica union is separate work.

**What exists vs planned:**

- Durable outbox and retries: `rust/client` `Session`; the desktop runtime drives them, a mobile
  worker is planned
- Hybrid logical clock timestamps with a peer-ID tie-break for ordering
  ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); the append-only log is
  planned
- Read receipts: `rust/client` `Session::mark_read` sends one, `Session::receive` applies one;
  no app UI yet ([read receipts](../design/read-receipts.md))
- Direct push, bootstrap relay, ACKs and typing indicators: *planned, not implemented*
- E2E encryption (MLS via `openmls`, [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)):
  implemented in `mls.rs`, [below](#e2e-encryption-mls); delivered by `rust/client`
- Sender authenticity: MLS signs every message with the sender device's certified key; the Rust
  chat event exposes this verified key alongside the author Peer ID for edit/delete ordering

## Message Model

```rust
pub struct Message {
    pub id: String,                  // UUID
    pub sender_id: String,           // Public key hash
    pub recipient_id: String,        // Public key hash
    pub text: String,                // Plaintext
    pub timestamp: Timestamp,        // HLC; JSON key "lamport_timestamp"
    pub created_at: i64,             // Unix millis
    pub status: MessageStatus,       // Pending, Sent, Delivered, Read, Failed
    pub encrypted_payload: Option<Vec<u8>>,  // reserved; not populated with ciphertext
    pub signature: Option<Vec<u8>>,          // reserved; not verified
}
```

**Status setters:** `mark_sent`, `mark_delivered`, `mark_read`, `mark_failed`. They do not check
the current state, so any transition is possible. Intended flow:

```text
Pending → Sent → Delivered → Read
    ↓
  Failed
```

## E2E Encryption (MLS)

`rust/messaging/src/mls.rs` wraps openmls 0.9 ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)):

- `Device::new(identity, device_key, generation)` makes a fresh MLS signature key and the
  identity-signed `DeviceCertificate` for the profile's device list; the signed certificate is the
  device's MLS basic credential.
- `Device::key_package(last_resort)` returns a KeyPackage valid for 30 days; a single-use one works
  for one Welcome only. `Session::keep_last_resort` keeps a reusable one published and replaces it
  after 20 days ([recovery boundary](../design/client-local-storage.md)).
- `Device::create_group(key_packages)` checks each KeyPackage (signature, suite 0x0001, current and
  at most 30 days long, certified credential) and returns the group with the Welcome for them;
  `Device::join(welcome)` checks the suite and every member's credential.
- `Group::encrypt` seals an application message; `Group::process` decrypts one and names the
  sender's Peer ID, or applies a commit after checking every new or updated leaf, and refuses a
  commit that changes the group's extensions and any bare proposal.
- `Group::export_secret` derives per-epoch secrets such as a call's frame key.

`verify_member` is the credential check: the certificate's signature under the
`dyapp/device-certificate/v1` label, the certified MLS key equal to the leaf's signature key, and
the certificate listed in the owner's profile among those the caller passes (the newest it holds).
Every call above takes those profiles, so a device dropped from the list is refused as a sender or
new leaf, and `Group::remove_revoked` commits its removal. `Group::missing_devices` names the
devices an owner lists without a leaf in the group, and `Group::add` commits them with a Welcome
([ADR 0040](../decisions/0040-device-linking.md)). `Session::add_own_devices` does this for every
group after a mailbox pass, the group of only the owner's devices included, which carries
`Body::PeerChange` (tag 13: a block, mute, pass or contact name with its HLC; accepted only in
that group). `Group::add_turn` orders the owner's devices in a group by device key,
and each waits `ADD_TURN_SECONDS` per earlier device, counted from when it first saw that profile
(`own_profile_seen` in `settings`), since two commits on one epoch would fork the group. The
desktop client takes the owner's profile from a node, ignoring one older than its own copy.

A device a newer profile no longer lists leaves every group the same way: `Group::remove_turn` orders
the members that stay by device key, contacts included, since a group of a contact and a lost device
has no own device left. Each waits `ADD_TURN_SECONDS` per earlier member, counted from when it first
saw the dropped device in that group (`dropped_leaf_seen/` in `settings`), and then commits with
`Session::remove_dropped_devices`. A member whose owner has no held profile stays, since an empty
cache says nothing. The desktop client refreshes one contact's profile per catch-up, in turn. If a
stale node copy drops a device by mistake, its owner's device adds it back.

Any own device revokes another with `publish::revoke_device`. It first closes the dropped device's
mailbox with an identity-signed `revoke` carrying the owner binding the device signed, which the
profile keeps ([bootstrap](bootstrap.md#mailbox-envelope)), so nodes stop serving it, and then
relists the profile without the device, which leaves a hidden profile hidden. A failed step is
retried by repeating both. `dyapp_client::owner_binding` signs that binding and `Node::close_mailbox`
sends the close.

`Session::own_device_revoked` tells a device that it was revoked: a profile past the last version
known to list it (`device_listed` in `settings`) leaves it out. A new device was never listed and an
older copy from a node proves nothing, and a withdrawn profile lists no device at all, so none of
them count. `Session::list_self` refuses to list a revoked device, so a draft taken from the newest
profile cannot bring it back. The owner's identity key is on every device, so this holds the app to
the rule and does not stop a stolen device.

The Linux client wipes a revoked device. When a node refuses its mailbox, `Session::revoked_or`
fetches the newest own profile, and only a refusal together with that signed profile counts: a fork
of the profile leaves the mailbox open, and a node cannot forge a profile. Catch-up then removes the
identity through the path of a manual removal, and says why. Revoking starts from the Tauri command
`revoke_device`. The Devices and linking screens, and the Android wipe, are planned.

A link record crosses in the new device's own mailbox: `Code::send` puts the sealed parts there and
`Invite::receive` reads them without MLS, among anything else the mailbox holds. An expired code
erases the X25519 key on the next look. `publish::adopt` then restores the owner's local state on a
fresh store, takes the newest profile with its photos as the draft, and `publish::relist` adds the
device. Storing the identity key under the platform store comes with the linking screen.

openmls keeps its state in memory. `Device::seal(key)` returns all of it (the signature key, unused
KeyPackages and every group) as nonce ‖ ChaCha20-Poly1305 ciphertext under a 32-byte key the client
keeps wrapped by the platform KeyStore; `Device::open(sealed, key)` restores the device and
`Group::load(device, id)` each group. The client must store a fresh seal after every call that
changes state and before sending its output, since an older state would reuse message keys. The
client and its store are planned.
The checked-in [old-provider state fixture](../security/openmls-curve-migration.md#current-provider-fixture)
tests that a future crypto-provider change can still read a saved signer, group and
unused KeyPackage. A replacement provider has not yet passed that test.

## Hybrid Logical Clock Ordering

`HybridClock` (`rust/messaging/src/hlc.rs`) stamps messages with a hybrid logical clock;
clones share state. A `Timestamp` packs wall-clock milliseconds into the high 48 bits and a
counter into the low 16, so comparing timestamps compares wall time, then the counter. On the
wire it stays a single integer under the old `lamport_timestamp` key.

- `now()` (on send): the current wall time with counter 0 if it is ahead of the last timestamp,
  otherwise the last timestamp plus one. A stalled or backward wall clock never moves the clock
  back; a full counter carries into the wall part.
- `observe(remote)` (on receive): the same rule applied to `max(last, remote)`, so the clock
  moves past every timestamp it has seen. A remote timestamp more than `MAX_CLOCK_SKEW_MS`
  (60 s) ahead of the local wall clock is rejected with `ClockSkew` and leaves the clock
  unchanged.
- `MessageOrdering` sorts by `(timestamp, sender peer ID, message id)`, so every peer orders the
  same set of messages the same way.

What this gives and what it does not:

- If event X causally precedes Y, then `ts(X) < ts(Y)`, and timestamps stay close to wall time.
  Concurrent messages are ordered by their timestamps, not detected as concurrent.
- A device whose clock runs up to 60 s fast can place its messages ahead of others' by up to
  that much; one further ahead has its messages rejected.
- Nothing calls `observe` yet: there is no receive path in `rust/messaging`.
- Ordering says nothing about delivery: a message that never arrives leaves no gap to detect.

## Mailbox Delivery — planned

Only the single-node mailbox is implemented ([bootstrap](bootstrap.md#served-protocol)): there
is no transport in `rust/messaging`. Decision:
[ADR 0009](../decisions/0009-message-delivery-and-storage.md).

1. The sender writes the message to the mailbox of each of the recipient's devices (each device
   is a separate MLS member): that device's replica nodes.
2. An online device keeps a connection to one of them and gets the message pushed at once;
   an offline one fetches it when it comes back.
3. The device acknowledges; the node forwards the ack to the other replicas, which drop the
   message, and the device drops duplicates by message id. The sender removes the message from
   its retry queue.

A node can hide part of a mailbox from its owner, so `dyapp_client::chat` checks it at a second
replica. Every MLS message names its epoch in the clear (`mls::epoch_of`); one from a later epoch
than the device's group (`Group::epoch`) means a commit is missing, so `chat::receive` reports
`Event::Gap` and leaves the envelope in the mailbox. On a gap, and once a day, the client calls
`chat::cross_check` with the node it fetched from and a node of another replica key: it applies
the union of both mailboxes, Welcomes first, then by epoch with each epoch's messages before its
commit, and returns the ids to ack at both. An envelope only the second node holds counts as a
fault of the first in the client's [node scores](bootstrap.md#replication-and-repair), unless the
first said more envelopes were waiting. Calling it daily is up to the app, planned with the
client store.

Planned limits (node settings): a message up to 100 KB, a mailbox up to 10 MB.

Clients never publish their addresses in the DHT. A direct connection that already exists (for
example during a call) may carry messages, as an optimisation only.

**Bootstrap never sees plaintext** when the client uses `rust/client` (see
[status](../security/encryption.md)):

- Message encrypted before sending to bootstrap
- Bootstrap only stores opaque blobs
- Decryption happens only on peer

## Message Types (planned wire format)

No serialization of these envelopes exists; `Message` derives serde but no format is chosen.

### Standard Message

```text
{
  "type": "message",
  "id": "uuid",
  "sender_id": "hash",
  "recipient_id": "hash",
  "encrypted_payload": "...",
  "signature": "...",
  "lamport_timestamp": 42
}
```

### Delivery ACK

```text
{
  "type": "ack",
  "message_id": "uuid",
  "recipient_id": "hash",
  "timestamp": 43
}
```

### Read Receipt

```text
{
  "type": "read",
  "message_id": "uuid",
  "timestamp": 44
}
```

## Testing

Unit tests in `rust/messaging/src/*.rs` cover message creation and status setters, clock
increment/observe/current/clone, and `MessageOrdering` sort; `merge()` has no tests. `src/mls/tests.rs` covers MLS ([what it checks](../security/encryption.md#current-state-verified-against-source)).
