# Client local storage design

The Rust client keeps active MLS handles in memory and seals them to SQLite. This design gives each
local identity durable chats, preferences and pending delivery while preserving the existing
[per-identity desktop slot](../development/desktop-identity-storage.md#multiple-identity-layout).
The [storage ADR](../decisions/0025-client-local-sqlite-storage.md) is accepted. The Linux desktop
uses the Store for profile drafts, privacy settings, likes and inbound mailbox work; chat text
screens remain planned. The Rust store foundation creates the identity-bound version-1 schema. Before the first release,
schema changes edit version 1 in place, without migration. It persists
the per-profile view-sending choice, local pass history, group ID catalog and like/unlike ledger.
The store can atomically stage a sealed MLS snapshot and exact signed outbox records,
reopen due records and schedule retries. It also commits an own interaction event, its HLC,
like/view state, sealed MLS snapshot and exact outbox writes in one transaction before network
delivery.
Inbox dedupe and pending ACKs are now atomic with a sealed MLS snapshot, and
`Session::receive` uses them for one-to-one chat and sends delivery receipts. The Linux desktop
calls it while polling the active identity's mailbox; the chat text UI remains planned.

## Ownership and location

The app supplies an absolute database path to the Rust store; the store never guesses an OS
directory or creates an identity key. On desktop, a new identity's database belongs under
`<app-config>/profiles/<slot>/client.db`; the virtual legacy identity uses
`<app-config>/client.db` without moving its key. Mobile apps supply a private per-profile
path in their own app data directory. The random local slot is a filesystem name, never a network
identifier. A database records the Peer ID derived from its selected identity key on first open
and refuses a different key thereafter. A separate database per profile avoids accidental
cross-profile joins and keeps local deletion scoped to the selected slot. The intended lifecycle
serializes writers per slot and drains old network work before switching. The current Tauri shell
serializes MLS commands but does not yet quiesce all in-flight commands on a switch or removal;
see the [desktop runtime audit](../architecture/ffi-design.md#current-desktop-runtime-ownership)
and [planned lifecycle](desktop-runtime-lifecycle.md).
Read-only multi-instance access is a later decision.

The identity key and the 32-byte MLS sealing key stay in the platform key backend, not SQLite.
`Device::seal` already encrypts the MLS signer, unused KeyPackages and all groups; only that sealed
blob enters the database. A mailbox write-token key is also a secret: its 32 bytes are
stored only under AES-256-GCM with the same sealing key, bound to the row's mailbox and day.
Message text, contacts and settings are plaintext in the local database under
the existing [device-at-rest threat decision](../security/threat-model.md#client-side-exposure).
The app uses its private directory permissions, platform file protection and backup exclusions;
desktop users need disk encryption for at-rest confidentiality. No user content, keys or Peer IDs
go into logs or migration diagnostics.

## Versioned records

SQLite databases use `user_version` 1 and one profile binding. There are no released clients or
supported existing databases, so pre-release schema changes replace this schema without a
migration. Existing development databases must be recreated or restored from a matching version.
Attachment bodies now carry a message-scoped logical ID and HLC. They remain in the transport
inbox until the planned attachment history projection stores descriptor metadata; a blob hash
cannot key a chat message because the same photo may be sent twice. No migration is needed
before the first app release.

These are the logical tables. Exact SQL and indexes belong to the storage implementation and its ADR.

| Table | Durable content | Key / invariant |
| --- | --- | --- |
| `profile_binding` | Derived Peer ID and store format | Exactly one identity per database; mismatch fails closed. |
| `mls_state` | One current sealed device blob | Replaced atomically with every operation that changes MLS state. |
| `conversation_groups` | Group ID and verified peer association | Registered with the sealed state and Welcome outbox; reopen rejects a missing group or peer mismatch. |
| `inbox` | Mailbox and envelope IDs, processed message or event, ack-pending bit | `(mailbox, envelope ID)` is unique; duplicate delivery never shows twice. |
| `chat_messages` | One current text or deletion placeholder per `(group ID, verified author Peer ID, logical message ID)` | The initial schema includes nullable original HLC, a device-local unread flag and a forwarded-text marker. A received forwarded text keeps the verified MLS sender as author; the marker survives restart and later edit/delete, and an ordinary text cannot reuse its logical ID. Accepted incoming text starts unread; own text stays read. `Store::mark_chat_read` clears the current unread rows for one peer across its MLS groups in one transaction without sending a read receipt. Edits and deletes keep the flag; `Store::clear_chat_pair` removes the flagged rows of every group for that peer in one transaction, preserving the conversation state and replay marks. Legacy tag-1 text has no original time; its `arrival_order` is a local, never-reused SQLite row ID for stable fallback ordering after Clear chat, not a sent timestamp or cross-device order. A received tag-12 original records its HLC, including after an edit/delete placeholder; conflicting time for the same logical ID is refused. SHA-256 of the original text identifies duplicate deliveries after an edit/delete without retaining another text copy. Store orders edits by `(HLC, device key, operation ID)` and treats delete as terminal, including when it arrives before the original. Chat operations and private interactions share one persisted HLC; rejected operations roll back clock changes. `Session::receive` uses `stage_chat_inbound` to commit this projection with the sealed MLS snapshot, inbox dedupe and pending ACK; the inbox retains only a body digest for text, forwarded text, edit and delete. The Store also provides `stage_chat_outbound` to commit an own text/forwarded text/edit/delete projection with exact outbox writes and sealed MLS state before network delivery; Session stages sends through this path. `Store::current_chat_messages` reads all current texts and deletion placeholders for one peer across cataloged groups, with optional original HLC and local unread flag; it orders legacy rows by local arrival and timed rows by original HLC, author, logical ID and group ID. `Store::chat_rows` reads per-peer state, latest current message/time and local unread count in one list query, keeping empty conversations after Clear chat. |
| `chat_operations` | Operation ID, target ID, sending device and SHA-256 body digest; replacement text only while the original is missing | Same group and author cannot reuse an operation ID with different content or device. At most 64 unresolved operations exist across the profile; the 65th is refused until an original resolves pending work. Pending text is cleared with that original. |
| `chat_cleared` | `(group ID, author Peer ID, message ID)` of each message Clear chat removed, no text | A text, edit or delete naming a mark is discarded and still ACKed; the mark outlives restarts. |
| `outbox` | Exact signed envelope bytes, mailbox, retry schedule and attempts; whether the row waits for a receipt and when a node first accepted it; which node accepted the latest copy and when; an optional expiry time; the chat group of a chat write | `(mailbox, envelope ID)` is unique; Clear chat drops unaccepted chat writes of its group; retry reuses bytes and ID. A receipt or presence row leaves on acceptance; any other row leaves on its receipt or 30 days after acceptance. A row past its expiry, set only for presence (2 hours after it was drafted), is given up unsent. |
| `settings` | Typed, versioned per-profile values such as view sending and the bounded mailbox fetch cursor | Per profile; the cursor row binds an opaque 1–64 byte position to one node Peer ID and an empty value means start at the head. The planned app lock is app-wide and its PIN verifier belongs in platform-protected storage, not this table. |
| `pass_history` | Peer identity and current local pass state | Never published or included in a signal. |
| `interaction_ledger` | Applied like, unlike and view events with sender device and exact payload | `(sender, event ID)` is unique; a replay with other bytes is refused. |
| `like_state` | Latest like or unlike per sender and target | Replaced only by a greater `(HLC, device, event ID)`. |
| `view_days` | UTC days on which one profile viewed another | One sent view per peer per day; received views are recorded regardless of the send setting. |
| `held_tokens` | Own token key per owner mailbox and UTC day, sealed, and the owner's signed WriteToken | `(mailbox, epoch)` is unique; a retry returns the same key. |
| `issued_tokens` | Token keys this profile signed, per contact, with a revoked bit | `(mailbox, token key, epoch)` belongs to one contact. |
| `peers` | Local block and mute flags, a local contact name and the device keys a verified contact listed per peer identity | Never published or included in a signal; absent means neither flag, no name and unverified. The local name, at most 64 characters of clean text, replaces the profile's public name; the public name is shown with the verified or unverified mark. A profile listing a device not seen at verification clears the mark until the safety number or QR code is compared again ([ADR 0004](../decisions/0004-identity-keys.md)). |
| `presence` | Per peer identity, when presence was last sent and the latest received presence HLC | Sent at most hourly to an unblocked match; received presence from a blocked peer or non-match is dropped before its HLC is observed ([presence](presence.md)). |
| `read_marks` | Per chat group and peer, the latest peer text a read receipt was sent for and the latest own text the peer has read | Order is `chat_messages` row order; a mark only moves forward, and a receipt from a blocked peer or naming an unknown text is dropped ([read receipts](read-receipts.md)). |

`send_profile_views` defaults to false when absent; `send_presence` (presence audience: on is
matches, off is nobody) defaults to true; `send_read_receipts` defaults to false. The stored
value is versioned and must be a single byte 0 or 1; an unknown version or value returns an
error rather than silently enabling signals. An absent pass record means no pass. Pass/undo
updates carry a persisted HLC timestamp; an older update cannot overwrite a newer local choice.
Each profile database owns its settings and pass history; neither is published to bootstrap.

`search_publish` holds the search publish window: the last time search nodes accepted the profile,
the time a `RATE_LIMITED` reply holds it until, both as big-endian Unix seconds, then the newest
signed profile not yet sent, if any. It is sent once 10 minutes have passed since the last send and
the hold has ended, also after a restart ([bootstrap](../architecture/bootstrap.md)); a held
tombstone is sent at once. `publish::hide` signs a search-only tombstone, holds it for search
and sets `profile_hidden` (a one-byte flag; absent means shown), and the next
`publish::publish` clears it. The last ordinary signed profile stays at store nodes, so someone
with the Peer ID can still find the device list and KeyPackages and start a chat. The hidden
draft advances to the tombstone version; unhide publishes a still-higher ordinary version to
both services. Identity removal instead
publishes a final tombstone to profile storage and search. A client that observed that final
profile-store tombstone refuses later versions; nodes retain tombstones for their configured
TTL under [ADR 0011](../decisions/0011-best-effort-deletion.md), so this is not a permanent
network-wide ban. Public fields remain fetchable by known Peer ID while merely hidden.

Signed-record sequence for one identity (each record is owner-signed):

| Action | Profile store | Search index | Local state |
|---|---|---|---|
| Visible at version `v` | Ordinary `v` with devices | Ordinary `v` | Shown |
| Hide | Keep ordinary `v` | Tombstone `v+1` | Hidden, with a pending withdrawal until a search node accepts it |
| Unhide | Ordinary `v+2` | Ordinary `v+2`, after the search rate window | Shown, with search publication pending until accepted |
| Remove while visible | Final tombstone `v+1` | Tombstone `v+1` | Key erased only after acceptance or explicit Remove anyway |
| Remove after Hide | Final tombstone `v+2` | Tombstone `v+2` | Key erased only after acceptance or explicit Remove anyway |

The two signed states are distinguished by where the tombstone is published; no new Profile
field is needed ([ADR 0036](../decisions/0036-search-visibility.md)). An ordinary profile after
a **search-only** tombstone is allowed. A client that has seen a **profile-store** tombstone
treats that identity as deleted and refuses later versions. Old or uncooperative nodes may
still serve a profile after deletion; tombstones
expire under the operator's configured TTL, so the UI promises withdrawal only on a
best-effort basis. A failed search withdrawal must remain pending and retryable rather than
claiming network disappearance.
`search::publish_due` sends it to the given search nodes; a `STALE` reply counts as accepted, and
with no node accepting, the longest `RATE_LIMITED` wait holds it. The Linux desktop shows the
minutes left (`Store::search_profile_minutes_left`) and sends a due profile while it runs and on
start. `search_heartbeat` (format 1)
holds the big-endian Unix second at which the next heartbeat is due: a random second of the UTC
day after a node last accepted one, so app opens do not set the time. `search::beat_due` sends
one with that day's proof of work only once it is due and the profile is not hidden, and a
`RATE_LIMITED` reply (two already that day) counts as accepted.

Like/unlike/view event ordering and match derivation belong to the
[interaction state](private-interactions.md); a match is derived from durable events rather than a
separately authoritative boolean. Chat history and outgoing work share this store so a failed
network attempt does not discard the user's message.

Opening verifies the bound identity and schema version. A different version, invalid binding
or corrupt database fails closed; it never silently creates an empty replacement at that path.
Pre-release schema changes do not migrate development databases. Once a released client needs a
schema change, decide its compatibility and backup policy then. Copying a live WAL database file
alone is not a consistent backup: stop and checkpoint the store first, or use SQLite's backup
mechanism. Restoring an old snapshot must not silently roll MLS state back over a newer one; the
app must require explicit recovery and re-establish sessions before sending.

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

For **send**, `Session::stage_text` creates a caller-visible logical ID, calls `chat::message` to
advance MLS state in memory, prepares one signed envelope for each device delivery, seals the
changed `Device`, and calls `stage_chat_outbound` with the own text projection and *all* prepared
envelopes before any network put. `Session::send_message` stages this first, then attempts
`Node::put_prepared`; `retry_due` sends queued writes after a failed put. `stage_edit` and
`stage_delete` similarly draft an operation ID/HLC and commit the new current projection with MLS
and exact outbox writes; the send wrappers then attempt delivery. `Session::chat_message` reads the
current state. The app has not wired this adapter yet. If sealing or staging fails, send nothing
and mark the in-memory session unusable. Reopen the last committed snapshot (or require a restart) before any further MLS
operation; retrying encryption on the advanced in-memory group could reuse state or diverge from
disk. A node's successful put (`Store::accepted`) only schedules the next delivery check, 1 h
later and doubling up to a day; it does not remove the outbox entry. `Store::receipted` removes it
when the recipient's receipt arrives, and `expire_outbound` gives it up 30 days after the first
acceptance. A receipt also names the nodes that dropped earlier writes to the same mailbox: rows
still waiting whose latest copy was accepted over a minute before the first acceptance of a
receipted write. Those rows become due at once and each copy blames its node only once;
`Session` lowers that node's score. `Store::accepted_by` tells a resend which node to try last.
Receipts themselves are staged with `stage_receipt` and leave on acceptance.

For **receive**, compute the device mailbox from its key and check `Store::has_inbox` before any
MLS processing. An already processed ID only needs a pending ACK retry. Process **one** new
envelope, seal the changed `Device`, then commit its inbox record, application event and ACK intent
with `stage_inbound` (or `stage_chat_inbound` for text, edit and delete). Only after that commit
may the UI show the event or `Node::ack` the ID. On a storage failure, neither show nor ACK it and
invalidate/reopen the in-memory session.
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

### Planned catch-up runtime

The platform runtime owns one catch-up job per active identity slot, sharing that slot's
`Session` lock with foreground chat commands. On desktop the job must hold the
[active-slot lease](desktop-runtime-lifecycle.md) through its blocking worker and network
calls. The WebView only requests an immediate pass and renders committed events; its timer
cannot own delivery because hidden views may be suspended. Mobile runtimes use the same
foreground contract when the app opens or resumes; OS background opportunities are
best-effort and are not a delivery guarantee.

Start a pass after the slot opens, when connectivity returns, when the app resumes or Chats
opens, and when a due outbox deadline arrives. A pass calls `retry_due`, then
`receive` and pending ACK handling for the active device; it schedules the next due
outbox write and a bounded mailbox check (the desktop currently polls every 60 seconds).
Keep only one pass per slot. A network failure schedules another pass with a bounded,
jittered delay; the durable outbox keeps its own per-envelope backoff, so restarting the
app may check connectivity immediately without rewriting send deadlines. Use the
client's existing 15-second per-request timeout and cap each pass's work so switching
can drain it. Never abort after an MLS mutation before its Store transaction settles.
A UI event is emitted only from a committed inbox row and carries the slot generation,
so a late result cannot render under another identity.

The first implementation can make one mailbox fetch per pass. It is **not** complete
backlog recovery: `Session::receive_page` now processes one bounded page through
the durable inbox and ACK path, but the desktop still calls legacy
`Session::receive`, which discards `more`. An unresolved first-page envelope can
hide later mail even across repeated desktop passes;
[the pagination checkpoint](../architecture/messaging.md#catch-up-checkpoints)
must wire continuation before claiming eventual catch-up. Cross-replica union
and reconciliation remain separate work.

| Owner | Benefit | Cost |
|---|---|---|
| Platform runtime job (chosen) | Runs while the desktop WebView is hidden and shares the active-slot lease and Session lock. | Requires wake scheduling and lifecycle wiring on each platform. |
| WebView timer | Reuses the existing Chats poll. | Suspension and slot switching can delay or misroute work. |
| Always-running service | Can attempt delivery while the app is closed. | Adds platform permissions, battery cost and lifecycle complexity without a portable guarantee. |

### Planned mailbox pagination

Bootstrap now accepts an optional cursor in the owner-signed `Fetch` and returns the
last emitted position in `MailboxResponse`. A cursor encodes a version, envelope class
and local SQLite sequence number. The node validates its shape and selects records
strictly after `(class, seq)` in its existing message-before-signal order. It sets
`more` when another record follows the returned byte/record-limited page. The cursor is
only a position, not an ACK or authorization token; the signed fetch and fresh
connection nonce remain mandatory. Old requests still fetch from the head. A node
that ignores the new field returns no next cursor. Legacy `Node::fetch` still
returns that first page; `Node::fetch_page` reports a non-advancing reply if
`more` is true, so the pagination caller cannot loop over page one.

The Store has one profile-bound cursor setting: `save_mailbox_cursor` records an
opaque 1–64 byte cursor with the responding node's Peer ID, `mailbox_cursor_for`
clears it on holder switch, and `clear_mailbox_cursor` resets to the head. Corrupt
format, Peer ID or length fails safely. `Session::receive_page` calls these APIs
after the page's per-envelope commits, ACK and receipt attempts; it returns
events and whether another page is due. A crash before cursor save replays a
page through inbox deduplication. A crash after save resumes after the
committed page. If the node changes, the client clears the cursor and starts from
the head; sequence numbers have no meaning across holders. Use a small page (at most
20 records per catch-up pass), and schedule another pass while `more` is true. Before
starting each next envelope or page, check the active-slot transition flag; never
interrupt an MLS/Store transaction. Once `more` is false, clear the cursor and
start the next scheduled check from the head, which also finds inserts with reused
or earlier sequence numbers. Deletion or eviction of the cursor row does not
invalidate the strict `>` position query. A new message inserted behind the cursor
is reached on a later head pass. A non-advancing or malformed cursor is an error,
not a reason for an unbounded refetch loop.

### KeyPackage recovery boundary

A device needs an available KeyPackage on a node before a new peer can create its MLS
group. The desktop currently publishes four single-use packages only when it publishes
the profile. Bootstrap consumes one before replying to a fetch, per holder and
allocation slot, and returns `NOT_FOUND` when stock is empty; it allows at most 20
unconsumed ordinary packages per device. `Session::refill` requests *write tokens*
from existing groups, not KeyPackages. The desktop's daily refill timer therefore
does not restore exhausted KeyPackages.

The target is to replenish a published device while it is active, without changing
its identity or creating a second device. Keep enough stock for an offline peer to
start a chat, and report temporary unavailability when stock or a fresh profile is
missing. Publishing the profile must precede package publication. Generate and seal
private package state before any node publish; a failed publish may leave unused
sealed keys but must not leave a public package without its private key. Never use a
newly generated package after a failed local transaction. A repeated node publish
of the same signed package must not unconsume it.

The chosen availability floor is one MLS last-resort KeyPackage per device, published
after the profile and initial single-use batch. Both the node and OpenMLS already support
reusing it for multiple Welcomes. Rotate it before its 30-day expiry while the owner is
active, and on the next foreground open after a longer offline period. Persist its
monotonically increasing revision and exact signed publication alongside the sealed
private MLS state before any network publish, then retry that publication after a lost
reply or restart. Use the existing node replication path and wait for its successful
publish response before treating the device as ready for new invitations.
A profile that no longer lists the device must not republish its package.

The current fetch API has no non-consuming stock count, so the client cannot know when
ordinary stock is low. The last-resort package keeps new invitations possible after
the four ordinary packages are consumed; it is not a claim that one-time package stock
has been refilled. Reusing one KeyPackage increases exposure if its private material is
compromised and means multiple Welcomes can target the same key. OpenMLS explicitly marks
and supports this mode; rotate at least before expiry and whenever the device certificate
changes. A blind fixed batch on every foreground start can hit the 20-package quota and
grow private state. A stale device certificate requires a refreshed verified profile
before an invite is retried.

| Refill option | Benefit | Cost or limit |
|---|---|---|
| One rotating last-resort package (chosen) | Uses the existing wire format, node replication and OpenMLS reuse; works after ordinary stock is consumed. | One private key can serve multiple Welcomes until rotation or expiry; it needs a durable revision and retry record. |
| Owner-authenticated stock inventory | Can top up ordinary packages to a low-water mark. | Adds a wire request, authorization and per-replica stock reconciliation; a count can change as peers consume packages. |
| Blind ordinary batch on every resume | No new wire request. | Can hit the 20-package quota and accumulate unused sealed keys without knowing whether nodes received stock. |

## Implementation checkpoints

1. Confirm the storage ADR and implement profile-bound open, the initial schema and failure tests.
2. Persist MLS state, inbox deduplication and exact outbound bytes in one transaction; test
   crashes immediately before and after commit, send and ack.
3. Add per-profile settings and pass history with restart/default tests. Add interaction events,
   the persisted interaction HLC and view-day deduplication (done).
4. Wire platform paths, permissions, backup exclusions and profile switching, then verify Linux
   and Android first; other platforms follow before release.

SQLite was chosen because the node already uses `rusqlite`, transactions cover the MLS/queue
boundary, and the client needs indexed history with room for future released-schema migrations. Independent JSON files would avoid
a client SQLite dependency but make atomic state-plus-outbox updates and crash recovery harder.
Encrypting the whole database would add a key lifecycle and recovery dependency; the current
threat decision instead relies on platform protection and desktop disk encryption. The identity
and MLS sealing keys remain protected separately either way.
