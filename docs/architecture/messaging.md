# Messaging Architecture (Offline-First)

## Overview

Message model, in-memory queue and Lamport clock in `rust/messaging`. There is no network
delivery yet: nothing sends, retries on a timer, or persists messages.

> ⚠️ Encryption and signatures are **stubs**: messages are plaintext today.
> See [Encryption & Security Status](../security/encryption.md).

**What exists vs planned:**
- In-memory queue (`VecDeque`): lost on restart, no persistence
- Retry bookkeeping (`retry_failed()`), called by the owner; no background worker
- Scalar Lamport timestamps for ordering (target: hybrid logical clock with a peer-ID
  tie-break and an append-only log, see
  [ADR 0010](../decisions/0010-data-sync-without-automerge.md))
- Direct push, bootstrap relay, ACKs, read receipts: *planned, not implemented*
- E2E encryption (MLS via `openmls`, [ADR 0005](../decisions/0005-openmls-end-to-end-encryption.md)): *planned, stubbed*
- Signatures for authenticity (Ed25519): *planned, stubbed*

## Message Model

```rust
pub struct Message {
    pub id: String,                  // UUID
    pub sender_id: String,           // Public key hash
    pub recipient_id: String,        // Public key hash
    pub text: String,                // Plaintext
    pub lamport_timestamp: u64,      // scalar Lamport clock
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

## E2E Encryption (target design, not implemented)

The code below exists, but `encrypt`/`decrypt` return input unchanged,
`sign` returns the message, `verify` only checks non-emptiness and
`derive_session_key` is XOR. The flows describe the **intended** design.

```rust
pub struct EncryptionConfig {    // held by E2EEncryption; values are labels only
    pub algorithm: String,            // "ChaCha20-Poly1305" (not used)
    pub key_exchange: String,         // "X25519" (not used)
    pub enable_frame_encryption: bool,
}
```

**Intended encryption flow:**
```
Plaintext Message
       ↓
[Encrypt with Session Key]
       ↓
Encrypted Payload + Nonce + Tag
       ↓
[Sign with Ed25519 Private Key]
       ↓
Payload + Signature + Public Key
       ↓
Serialize & send to peer
```

**Intended decryption flow:**
```
Received Message
       ↓
[Verify Signature with Sender's Public Key]
       ↓
[Decrypt with Session Key]
       ↓
Plaintext Message
```

**Intended session key derivation** (today: XOR placeholder):
```
Shared Secret (X25519) + Salt
       ↓
[HKDF-SHA256]
       ↓
32-byte Session Key
```

## Lamport Clock Ordering

Current code. The target replaces it with a hybrid logical clock and a peer-ID tie-break
([ADR 0010](../decisions/0010-data-sync-without-automerge.md)).

`LamportClock` is a single `u64` counter shared between clones:
- `increment()`: `c = c + 1`, returns `c` (used when sending)
- `observe(remote)`: `c = max(c, remote) + 1` (on receive; always advances)
- `merge(other)`: same rule with another clock; `current()`, `reset()`

```
Peer A                           Peer B
  | send msg1 (A: 1)               |
  |------------------------------->| observe(1): B = max(0,1)+1 = 2
  | send msg2 (A: 2)               |
  |------------X (delayed)         |
  |                                | send msg3 (B: 3)
  |<-------------------------------|
  | observe(3): A = max(2,3)+1 = 4 |
  |            (msg2 arrives late) |
  |------------------------------->| observe(2): B = max(3,2)+1 = 4
```

What this gives and what it does not:
- If event X causally precedes Y, then `ts(X) < ts(Y)`. The converse is false: msg2 (2) and msg3
  (3) above are concurrent, and a scalar clock cannot detect that (a vector clock could).
- `MessageOrdering` sorts by timestamp with a stable sort, so equal timestamps keep insertion
  order. That order can differ between peers; a deterministic tie-break (for example by sender id)
  is not implemented.
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

Planned limits (node settings): a message up to 100 KB, a mailbox up to 10 MB.

Clients never publish their addresses in the DHT. A direct connection that already exists (for
example during a call) may carry messages, as an optimisation only.

**Target: bootstrap never sees plaintext** (not true today, see [status](../security/encryption.md)):
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
`merge()` have no tests. The encryption tests exercise the stubs only (not a security test).

## Configuration

Planned options (config fields exist, behaviour is not implemented):
- **Enable frame encryption**: encrypt video frames (more secure, more CPU)
- **Algorithm agility**: switch between ChaCha20 and AES-256 (future)
- **Signature verification**: always verify or skip (security vs speed)

`EncryptionConfig::default_secure()` sets the labels "ChaCha20-Poly1305" / "X25519"; no algorithm runs.
