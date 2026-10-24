# Messaging Architecture (Offline-First)

Authored Rust APIs document local status changes, plaintext retention and queue retry
bookkeeping. Queueing and attaching encrypted bytes do not by themselves encrypt,
verify receipts or send messages; the service's sender label is currently `self_id`.

## Overview

Message model, in-memory queue and hybrid logical clock in `rust/messaging`. The queue is not
wired to the network: it sends, retries on a timer and persists nothing.

Delivery of MLS chats lives in `rust/client` (`dyapp-client`): `Node` speaks the profile,
KeyPackage and mailbox protocols to one bootstrap node, and `chat` sends every Welcome, commit
and padded message as MLS bytes, one envelope per member device. Calls that change MLS state
return the deliveries instead of sending them, and `chat::receive` returns the envelope ids to
ack, so the client seals the state first. The app does not use it yet. See
[Encryption & Security Status](../security/encryption.md).
`prepare_put` can sign a mailbox write before network activity and return exact bytes, mailbox
and ID. `PreparedEnvelope::from_bytes` verifies a stored copy; `Node::put_prepared` reuses that
record on retry. `Node::put` still prepares and sends in one call for existing callers. The
durable outbox and receipt handling remain planned.
The planned like and introduction signals need shorter plaintext padding than ordinary chat;
the measured envelope sizes and 1 KiB limit are documented under
[bootstrap mailbox envelopes](bootstrap.md#mailbox-envelope).
The planned like, unlike and view payload and match state are specified in the
[private interaction design](../design/private-interactions.md).
`rust/client::interactions` now encodes and strictly decodes the 26-byte v1 plaintext for those
three kinds. A Rust test sends it through the existing MLS chat path and decodes the result. The
client still lacks a short-padded signal sender, persistent interaction state and app integration;
the codec alone does not publish a like or compute a match.
`rust/client::store::Store::open` now creates or reopens an identity-bound, version-1 SQLite
database at an absolute path supplied by the app. It refuses a different identity, unknown
unversioned objects, corrupt data and future schema versions. WAL uses full synchronous commits.
The store persists the view-sending choice and local pass/undo state with a privacy-preserving
default of no view signals; the schema also reserves inbox, outbox and MLS state tables that no
chat operation uses yet. See the [client storage design](../design/client-local-storage.md).
The store can now commit a caller-provided sealed MLS snapshot and exact prepared mailbox writes
in one SQLite transaction, reopen due writes after restart and schedule a later retry. A conflict
on an existing mailbox/ID with different signed bytes rolls the whole transaction back. The
chat API is not wired to this store yet; no receipt currently removes a pending outbox row.
It can also commit an incoming envelope ID, caller-provided application payload and new sealed MLS
snapshot in one transaction. The caller checks the ID before applying MLS; after commit it can
acknowledge pending IDs, and a restart reopens the ACK queue. Duplicate IDs never overwrite the
saved MLS snapshot. Wiring this sequence into the network receive path is still planned.

**What exists vs planned:**
- In-memory queue (`VecDeque`): lost on restart, no persistence
- Retry bookkeeping (`retry_failed()`), called by the owner; no background worker
- Hybrid logical clock timestamps with a peer-ID tie-break for ordering
  ([ADR 0010](../decisions/0010-data-sync-without-automerge.md)); the append-only log is
  planned
- Direct push, bootstrap relay, ACKs, read receipts: *planned, not implemented*
- E2E encryption (MLS via `openmls`, [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)):
  implemented in `mls.rs`, [below](#e2e-encryption-mls); delivered by `rust/client`, not by the
  queue
- Sender authenticity: MLS signs every message with the sender device's certified key

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
```
Pending → Sent → Delivered → Read
    ↓
  Failed → (retry_failed) → Pending
```

## Offline Queue

```rust
pub struct QueueEntry { pub message: Message, pub retry_count: u32, pub next_retry_at: i64 }

pub struct MessageQueue {
    entries: VecDeque<QueueEntry>,  // in memory only
    max_retries: u32,               // 5, not configurable
}
```

**Operations** (all synchronous; none sends anything):
- `enqueue(message)`: append with `retry_count = 0`, `next_retry_at = 0`
- `dequeue()`, `peek()`, `size()`, `is_empty()`, `clear()`
- `get_pending()`: clones of messages with status `Pending`
- `mark_delivered(id)`: set status `Delivered`; `Err` if the id is unknown
- `remove_delivered()`: drop `Delivered` and `Read` entries
- `retry_failed()`: for each `Failed` entry with `retry_count < 5` and `next_retry_at <= now`:
  increment `retry_count`, set status back to `Pending`, set
  `next_retry_at = now + 1000 ms × 2^(retry_count − 1)`, and return a clone

**Backoff, as computed:** the first retry is allowed immediately; after retries 1–4 the next one
is allowed no earlier than 1, 2, 4 and 8 s later (15 s minimum span for 5 retries). The 16 s value
set after the 5th retry is never used, because the limit is reached. These are earliest times:
something must call `retry_failed()`, and there is no queue method to mark an entry `Failed`, so
only a message enqueued already `Failed` can be retried today.

`MessagingService` (in `lib.rs`) wraps the queue: `send_message` increments the clock and enqueues
a message from the hard-coded sender `"self_id"`; it does not deliver it.

## E2E Encryption (MLS)

`rust/messaging/src/mls.rs` wraps openmls 0.9 ([ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)):

- `Device::new(identity, device_key, generation)` makes a fresh MLS signature key and the
  identity-signed `DeviceCertificate` for the profile's device list; the signed certificate is the
  device's MLS basic credential.
- `Device::key_package(last_resort)` returns a KeyPackage valid for 30 days; a single-use one works
  for one Welcome only.
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
new leaf, and `Group::remove_revoked` commits its removal. The owner's other device also closes
the dropped device's mailbox with an identity-signed `revoke` carrying the owner binding the device
signed at linking ([bootstrap](bootstrap.md#mailbox-envelope)), so nodes stop serving it.
`dyapp_client::owner_binding` signs that binding, `Node::close_mailbox` sends the close, and
`chat::revoked` tells a device that its owner's newest profile no longer lists it, so it wipes its
keys and MLS state. The linking screen and the wipe itself are planned with the client apps.

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
is no transport in `rust/messaging`, and nothing wires the queue to it. Decision:
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

```
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

```
{
  "type": "ack",
  "message_id": "uuid",
  "recipient_id": "hash",
  "timestamp": 43
}
```

### Read Receipt

```
{
  "type": "read",
  "message_id": "uuid",
  "timestamp": 44
}
```

## Testing

Unit tests in `rust/messaging/src/*.rs` cover message creation and status setters, queue
enqueue/dequeue, pending filter, `mark_delivered`, `remove_delivered`, clock
increment/observe/current/clone, and `MessageOrdering` sort. `tests/integration_tests.rs` checks only the first `retry_failed()` call (re-enqueueing resets `retry_count`); later backoff steps and
`merge()` have no tests. `src/mls/tests.rs` covers MLS ([what it checks](../security/encryption.md#current-state-verified-against-source)).
