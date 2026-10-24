# Client local storage design

The Rust client currently keeps MLS state and mailbox work in memory. This design gives each
local identity durable chats, preferences and pending delivery while preserving the existing
[per-identity desktop slot](../development/desktop-identity-storage.md#multiple-identity-layout-planned).
The [storage ADR](../decisions/0025-client-local-sqlite-storage.md) is accepted. Nothing on this
page is wired into an app yet. The Rust store foundation creates the identity-bound version-2
schema and migrates version 1 in place after checking the identity binding. It persists the
per-profile view-sending choice, local pass history and group ID catalog; chat, MLS and outbound
mailbox calls are not wired into the app yet. The store can atomically stage a sealed MLS
snapshot and exact signed outbox records, reopen due records and schedule retries. Inbox dedupe
and pending ACKs are now atomic with a sealed MLS snapshot, and `Session::receive` uses them for
one-to-one chat; the app does not call the session yet, and delivery receipts remain planned.

## Ownership and location

The app supplies an absolute database path to the Rust store; the store never guesses an OS
directory or creates an identity key. On desktop, a new identity's database belongs under
`<app-config>/profiles/<slot>/client.sqlite3`; the virtual legacy identity uses
`<app-config>/client.sqlite3` without moving its key. Mobile apps supply a private per-profile
path in their own app data directory. The random local slot is a filesystem name, never a network
identifier. A database records the Peer ID derived from its selected identity key on first open
and refuses a different key thereafter. Profile switching closes the previous store and stops its
network work before opening the next one. A separate database per profile avoids accidental
cross-profile joins and keeps local deletion scoped to the selected slot. The app serializes
writers for each slot; read-only multi-instance access is a later decision.

The identity key and the 32-byte MLS sealing key stay in the platform key backend, not SQLite.
`Device::seal` already encrypts the MLS signer, unused KeyPackages and all groups; only that sealed
blob enters the database. Message text, contacts and settings are plaintext in the local database
under the existing [device-at-rest threat decision](../security/threat-model.md#client-side-exposure).
The app uses its private directory permissions, platform file protection and backup exclusions;
desktop users need disk encryption for at-rest confidentiality. No user content, keys or Peer IDs
go into logs or migration diagnostics.

## Versioned records

The initial SQLite schema has one transactionally migrated `user_version`, one profile binding,
and these logical tables. Exact SQL and indexes belong to the storage implementation and its ADR.

| Table | Durable content | Key / invariant |
| --- | --- | --- |
| `profile_binding` | Derived Peer ID and store format | Exactly one identity per database; mismatch fails closed. |
| `mls_state` | One current sealed device blob | Replaced atomically with every operation that changes MLS state. |
| `conversation_groups` | Group ID and verified peer association (version 2) | Registered with the sealed state and Welcome outbox; reopen rejects a missing group or peer mismatch. |
| `inbox` | Mailbox and envelope IDs, processed message or event, ack-pending bit | `(mailbox, envelope ID)` is unique; duplicate delivery never shows twice. |
| `outbox` | Exact signed envelope bytes, mailbox, retry schedule and attempts | `(mailbox, envelope ID)` is unique; retry reuses bytes and ID. |
| `settings` | Typed, versioned values such as view sending and app lock preference | Per profile; documented defaults for absent values. |
| `pass_history` | Peer identity and current local pass state | Never published or included in a signal. |

`send_profile_views` defaults to false when absent. The stored value is versioned and must be a
single byte 0 or 1; an unknown version or value returns an error rather than silently enabling
signals. An absent pass record means no pass. Pass/undo updates carry a persisted HLC timestamp;
an older update cannot overwrite a newer local choice. Each profile database owns its settings
and pass history; neither is published to bootstrap.

Like/unlike/view event ordering and match derivation belong to the
[interaction state](private-interactions.md); a match is derived from durable events rather than a
separately authoritative boolean. Chat history and outgoing work share this store so a failed
network attempt does not discard the user's message.

Migration starts a transaction, verifies the bound identity and current schema version, applies
one monotonic version step, and commits only after its integrity checks. A future version, invalid
binding or corrupt database fails closed with a recoverable error; it never silently creates an
empty replacement at that path. Before a destructive migration, the app must offer a consistent
backup. Copying a live WAL database file alone is not a consistent backup: an app-provided export
must use SQLite's backup mechanism or stop and checkpoint the store first. Restoring an old
snapshot must not silently roll MLS state back over a newer one; the app must require explicit
recovery and re-establish sessions before sending.

## Crash and network ordering

```text
Send:    create MLS ciphertext → seal changed MLS state → commit seal + exact outbox bytes
         → send exact envelope → mark delivery after the required receipt
Receive: decrypt/process → seal changed MLS state → commit seal + inbox/dedupe + pending ack
         → acknowledge mailbox → mark ack complete
```

The store commits before each network side effect. A crash after a send can replay the same
envelope ID and bytes; the mailbox put is idempotent for that ID. A crash after processing but
before ack sees the durable inbox row and acknowledges without showing the message or applying
MLS state twice. The database transaction is the boundary between MLS state and queue state;
neither may be saved alone. Outbox entries remain until the protocol's delivery receipt, not just
a node's acceptance, and use bounded backoff. Mailbox expiry and signal eviction still mean a
recipient can miss an event; reconciliation remains necessary.

### Session recovery contract

The app owns one active `Device`, its loaded `Group` handles, the selected identity slot, the MLS
sealing key and the SQLite `Store` together. `stage_group` atomically records a new group's ID,
its verified peer, the sealed Device and all prepared Welcome writes. On restart, `load_groups`
uses the catalog to restore every group from the unsealed Device and refuses a missing group or
one without its recorded peer. It opens the identity key first, then the matching database, then
`Device::open` with the key unwrapped from the platform backend and `load_groups` for each
conversation. A missing or inaccessible key does not start network work. Never keep a
group handle from the previous device after reopening a snapshot.

For **send**, `Session::send_message` calls `chat::message` to advance MLS state in memory. It
prepares one signed envelope for each device delivery, seals the changed `Device`, and calls
`stage_outbound` with *all* prepared envelopes before `Node::put_prepared`. The app has not wired
this adapter yet. If sealing or staging fails, send nothing and mark the in-memory
session unusable. Reopen the last committed snapshot (or require a restart) before any further MLS
operation; retrying encryption on the advanced in-memory group could reuse state or diverge from
disk. A node's successful put only schedules the next delivery check; it does not remove the
outbox entry. Receipt processing is a separate protocol step.

For **receive**, compute the device mailbox from its key and check `Store::has_inbox` before any
MLS processing. An already processed ID only needs a pending ACK retry. Process **one** new
envelope, seal the changed `Device`, then commit its inbox record, application event and ACK intent
with `stage_inbound`. Only after that commit may the UI show the event or `Node::ack` the ID. On a
storage failure, neither show nor ACK it and invalidate/reopen the in-memory session.
`Session::receive` implements this per envelope; the batch `chat::receive`/`apply_all` must not be
wired to `stage_inbound`, because it applies several MLS mutations before returning ACK IDs. A
Welcome's group and its single other owner enter the catalog in the same commit; a group with any
other membership is refused until multi-device sync or group chats exist.

`Event::Gap` and `Event::UnknownOwner` stay unacknowledged and unrecorded until the missing commit
or verified profile arrives. Unknown group messages likewise remain in the mailbox. Refused
envelopes can be ACKed only after a durable refusal/deduplication marker (`InboxKind::Refused`,
with the previous MLS snapshot kept); the UI must not show their bytes. Cross-check of a second
replica must pass its union through the same per-envelope deduplication path; it does not yet.
The current fetched `Envelope` omits its class, so private signal dispatch needs a separate
transport change; this chat checkpoint handles ordinary MLS messages only.

The first integration tests must inject a store failure after MLS send and receive mutations,
reopen the last snapshot, and prove the unsent/outstanding envelope can be processed without
duplicate display or premature ACK. They must also restart after a committed send, retry exact
signed bytes, and restart after a committed receive before ACK.

## Implementation checkpoints

1. Confirm the storage ADR and implement profile-bound open, schema migration and failure tests.
2. Persist MLS state, inbox deduplication and exact outbound bytes in one transaction; test
   crashes immediately before and after commit, send and ack.
3. Add per-profile settings and pass history with restart/default tests. Add interaction events
   and view-day deduplication under the separate private-interaction task.
4. Wire platform paths, permissions, backup exclusions and profile switching, then verify Linux
   and Android first; other platforms follow before release.

SQLite was chosen because the node already uses `rusqlite`, transactions cover the MLS/queue
boundary, and the client needs indexed history and migrations. Independent JSON files would avoid
a client SQLite dependency but make atomic state-plus-outbox updates and crash recovery harder.
Encrypting the whole database would add a key lifecycle and recovery dependency; the current
threat decision instead relies on platform protection and desktop disk encryption. The identity
and MLS sealing keys remain protected separately either way.
