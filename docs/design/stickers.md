# Sticker packs

Status: required for v0.0.1. The bounded in-memory TAR reader exists in
`rust/client/src/sticker_tar.rs`. `sticker_webp::check_header` bounds the RIFF envelope,
canvas, embedded frame dimensions, animation count and duration, and rejects EXIF/XMP.
The header check alone neither validates compressed pixels nor makes input safe to
render. `sticker_webp::decode::first_frame` runs it before image-webp 0.2.4, rechecks
decoder dimensions/frame count/duration/output size, applies a 16 MiB decoder budget,
catches decoder panics, rejects a fully transparent first canvas and returns RGBA8.
It decodes only the first frame; later compressed frames are not decoded for playback.
Some library allocations ignore the budget, so this is not process isolation.
[ADR 0045](../decisions/0045-sticker-webp-decoding.md) records that trade-off.

`sticker_manifest::check_files` structurally checks a decoded
manifest against exact root files: safe unique names, count/byte bounds, SHA-256,
original manifest identity and manifest-order output. Its aggregate plaintext bound
is necessary but not sufficient for the 6 MiB sealed-record limit; protobuf and
sealing overhead still need checking. It does not parse JSON, check display text or
validate WebP pixels, and cannot authorize storage/rendering. The typed
`sticker_manifest::json::parse` stage checks format, duplicate known fields, clean NFC
names and one-grapheme labels (both at most 64 code points), hashes and bounded entry
count. Parsed entries stay bound to the original bytes when invoking file checks;
unknown formats ask to update. Unknown JSON fields are ignored for extensions.
[ADR 0044](../decisions/0044-sticker-manifest-validation.md) records the parser choices.
`sticker_pack::import_tar` and `from_peer` build immutable protobuf pack records
through the shared JSON/file/hash/first-frame validator. Original manifest bytes
are field 1; field 2 repeats files in manifest order. Peer field lengths/counts are
scanned before allocation. The full encoded record plus chunked AES-GCM tags must
fit 6 MiB. RGBA is checked one frame at a time and discarded during pack validation.
Pack storage, the composer's collection manager and tag-14 receive are implemented;
Session supports saved-pack upload reuse, metadata sends and guarded pack-control receive;
automatic pack transfer and image UI remain planned. Decoder CPU,
representative-file measurements, successful decoder fuzz CI runs and release-size
verification remain required. The WebP fuzz target and static/animated/corrupt seeds
are configured; its 256 KiB input cap covers the allowed compressed-byte range.
The numbers marked *assumed* need measurement and may change.
The reader caps the archive envelope at 8 MiB (including TAR padding), files at
121 including the 64 KiB manifest, and sticker file bytes at 256 KiB. The separate
6 MiB sealed-pack limit will be enforced by pack validation. GNU tar fixture bytes
exercise archive parsing only; the fixture sticker payload is deliberately not WebP.
The scheduled sticker_tar fuzz target mutates this archive seed and checks accepted
file-count, path and byte bounds; its CI execution is still required before release.

A user imports a sticker pack from a file, sends stickers from it in a chat, and a peer can save
the whole pack and send from it too. Every sticker travels as an ordinary sealed
[attachment](../architecture/messaging.md) through the nodes. The transport does not change.

## Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Source | Import only; the format has room for built-in packs, v0.0.1 ships none | All content is untrusted, so one validator covers every path |
| Import file | Uncompressed TAR with `manifest.json` at its root | Authors have `tar`; no new dependency; no decompression bombs |
| Peer exchange | Protobuf record rebuilt from the manifest and files | Peer input never reaches the TAR parser, TAR metadata stays on the importer |
| Delivery | Single sticker and whole pack as separate attachments | A recipient downloads only what they see, a pack only on a tap |
| Image format | Static or animated WebP, decoded in the Rust core | One decoder, RGBA to every UI, OS image decoders never see peer bytes |
| Animation | v0.0.1 shows the first frame on every client | Playback costs about 92 MiB and 160 MiB/s of IPC per sticker at the limits below |
| Rights | No confirmation, setting or per-pack block | The author decides what they send; the existing peer block is the protection |
| Identity | Pack ID is the SHA-256 of the manifest bytes; a pack is immutable | A new version is a new pack |

Automatic pack redistribution is an accepted risk: a person may spread a pack they have no rights
to. A legal review is not a release condition.

### Alternatives rejected

- **Peer-to-peer transfer from whoever is online.** There is no client-to-client channel and
  [clients never publish addresses](../architecture/messaging.md); it would add a new transport.
- **Direct download links.** A sender could point every recipient at a host of their choice, which
  turns a chat into a traffic source against that host and reveals recipients' IP addresses to it.
- **ZIP.** It needs a new dependency and a defence against compression bombs. WebP is already
  compressed, so ZIP saves nothing.
- **Telegram files as they are.** TGS and WebM would need two more decoders. Authors convert
  before import.
- **Pack only, or a sticker only by hash.** Pack-only makes the first sticker cost megabytes;
  hash-only leaves a peer with an empty message until they ask for something.

## Pack format

```sh
tar -cf cats.tar manifest.json *.webp
COPYFILE_DISABLE=1 tar -cf cats.tar manifest.json *.webp   # macOS
```

```json
{"format":1,"name":"Cats","stickers":[{"file":"cat.webp","sha256":"...","emoji":"😺"}]}
```

An unknown `format` fails with "update the app". The `name` and `emoji` are untrusted: they follow
the display-safety rules of profile text ([ADR 0026](../decisions/0026-profile-text-rules.md)
and `check_name`), the name is bounded and `emoji` is one grapheme.

The reader lives in the core and accepts only regular files at the archive root. Links, folders,
files the manifest does not list, and any other entry, including macOS `._*`, fail with an error
naming the file. PAX metadata records are skipped.

### Limits

| Limit | Value |
|-------|-------|
| One sticker | at most 256 KiB, longest side at most 512 px |
| Animation | at most 90 frames and 3 s |
| Pack | sealed size at most the node media limit of 6 MiB |
| Stickers in a pack | at most 120 (*assumed*) |
| Manifest | at most 64 KiB |
| First frame | not fully transparent |
| Metadata | EXIF and XMP refused |
| Saved packs on a device | 300 MiB; over it import or save fails with an error, nothing is evicted |
| Individual chat stickers | Same storage settings and cleanup as other chat media; no separate sticker quota |
| Single sticker uploads | 16 MiB charged per sender (*assumed*) |

A pack over 6 MiB fails with the overflow in the message, and its author splits it.

## Validation

One validator runs before anything is stored, for an import, a pack from a peer and a single
sticker. It checks the hashes against the manifest, the size and WebP parameters, reading the
header before any frame buffer is allocated. Decoding runs in the background under `catch_unwind`.
Only RGBA leaves the core. `image-webp` becomes a new direct dependency; its memory limit does not
cover every allocation, so a hang or a large allocation stays a residual risk and the header check
carries the load.

The TAR reader and the pack validator get fuzz targets and corrupt fixtures, and real archives from
GNU tar and macOS bsdtar are test fixtures.

## Wire

Three MLS application variants use the next free outer tags; nodes see only sealed blobs.
Tag 14 sticker decoding and authenticated session receive are implemented. Pack controls
15/16 now route through durable per-chat authorization on receive. Their
`chat::pack_control` payload codecs bound input to 2 KiB before protobuf allocation,
require 32-byte manifest IDs and reuse the shared attachment codec. An answer has no preview
or release capability, zero nested message ID/HLC and at most 6 MiB including GCM tags.
A valid descriptor alone never authorizes a download. Tag 13 stays private own-device `PeerChange`.

| Tag | Body |
|-----|------|
| 14 sticker | 16-byte logical ID, HLC, pack ID, sticker SHA-256, emoji, pack name, optional attachment descriptor |
| 15 pack request | pack ID |
| 16 pack answer | pack ID, attachment descriptor |

The tag-14 payload leaves room for an optional thumbnail. No client release exists yet, so no older client
needs a stub.

The existing tag-6 descriptor conversion and validation live in
`rust/client/src/chat/attachment_wire.rs`, shared within the chat codec. Sticker-body
integration uses this same validation boundary.
`chat::sticker::Sticker` now encodes/decodes a payload bounded at 2 KiB before protobuf
allocation. It uses the manifest's clean NFC name and single-grapheme label rules,
fixed-size IDs, a signed-range HLC and an optional existing attachment descriptor.
The nested descriptor's message ID/HLC must equal the sticker's; its release secret
is zero, preview is empty and plaintext size is 1..=256 KiB. A mismatched plaintext
hash preserves fallback metadata but `download()` returns no descriptor. Compressed
bytes go through `Sticker::first_frame`: it bounds the input, binds its SHA-256 to
the message and returns only validated RGBA8 through the shared WebP decoder. It also
accepts matching bytes already available from a saved pack when the descriptor is absent
or has a different hash; that descriptor still cannot authorize fetching. Outer tag 14
uses this validator before admission. Session receive applies attachment conversation guards
and commits sticker metadata, HLC, inbox/ACK and the MLS snapshot in one transaction.
Pre-match media is acknowledged without projection; forwarded stickers are refused.
Body serialization is permissive like other bodies; encryption validates by decoding first.
Session sticker sends now commit fresh ID/HLC, common-media delivery state, exact outbox
writes and MLS snapshot atomically. Its template ID/HLC are ignored; malformed labels or
descriptors fail before encryption. Sending requires an open, unblocked pair conversation.
Own stickers support terminal Delete through the normal operation path, never Edit.
Saved-pack upload/reuse is implemented; pack request/answer authorization remains planned. The common SQLite media table
has a checked kind discriminator and shared insertion checks. The Store writes and reads
sticker payloads with optional descriptors, checks verified context and cross-kind ID conflicts,
and preserves unread state on replay/restart. Clear chat removes their rows and prevents replay
from restoring them. Common originals references include only hash-matching descriptors, so
no sticker-only cache is needed. Photo queries exclude sticker rows; merged history includes typed stickers. Desktop/UniFFI metadata omit transport secrets;
Desktop and Android render a localized emoji/name fallback without fetching or edit/forward actions.
Desktop chat-list previews now include the newest sticker and count its unread state;
image rendering, mobile list previews and Add pack actions remain planned. At the Store boundary, tag-9 deletion of a
stored sticker now removes its media row and leaves the shared terminal placeholder. Replays
cannot restore it; an original arriving after deletion binds its digest/HLC and releases pending
operation slots without restoring content. Edits to a stored sticker fail; early edits are discarded
when the original is identified as a sticker. Send/image UI wiring remains planned.

Each descriptor sent to peers carries a zeroed `release_secret`. One blob serves many chats, so a
recipient must not be able to release it for the others; the sender keeps the real secret. The
codec already takes any 32 bytes there.

The pack blob holds a protobuf record of the manifest bytes and the files in manifest order.

## Flows

### Send

The first send of a sticker uploads it as its own attachment and waits for the upload, as a photo
does. The descriptor is stored and reused for every later send, in any chat, and the blob is
uploaded again when under a day of the 168 h retention is left. Node quota charges small blobs five
times in its internal accounting units; a 256 KiB sticker including GCM tags costs 1,311,040
units. The node scales its owner quota by the same factor. The separate application cap is
16 MiB in charged units. Past that cap a sticker goes without a descriptor and the peer sees
the fallback. The Store ledger implements atomic reservation, restart-safe reuse and final-day
refresh: confirmed descriptors last seven days, uncertain uploads reserve eight days. Each
refresh has a random reservation token so a late completion cannot confirm its successor.
The same ledger now supports whole-pack records under a separate kind, with a 6 MiB sealed
bound and node owner quota. The 16 MiB application cap applies only to individual stickers.
Kinds have distinct keys and completion checks even for equal plaintext hashes; pack descriptors
have zero shape/preview and application/octet-stream. Descriptor APIs validate metadata only;
callers must first load a validated immutable Pack and bind reconstructed ciphertext to its bytes.
Every pending retry renews the uncertainty window before network I/O; repeated completion
cannot extend confirmed retention. Tests cover late retry, corrupt rows and simultaneous
reservations for the last quota slot. Expired ledger metadata can be pruned; saved packs
and chat history are untouched.
`Session::send_pack_sticker_to` now connects this ledger to the shared photo attach/put
transport. It checks owner and conversation before reserving, reconstructs only hash-matching
ciphertext and strips the release secret before staging. A failed upload retains its reservation
without adding a chat message. Confirmed reuse and quota fallback still use fresh message IDs.
Desktop picker/command wiring and live node integration coverage remain planned.

### Receive

The client looks the hash up in its packs and cache. If absent, it fetches under the
`attachment::auto_download` rules, checks that the descriptor's plaintext hash equals the sticker
hash, validates and caches. A mismatch or a failure shows the fallback: emoji, pack name and an "Add
pack" button. The fallback also covers an expired blob, auto-download off and a sender over budget.

### Save a pack

The shared Store now saves validated packs as sealed identity-local files, with bounded
reads, idempotent saves and a 300 MiB quota without eviction. Wrong-key and corrupt files
are preserved. Desktop protects the sealing secret even before the first MLS session.
The Store also lists validated names, sticker counts and encrypted sizes in PackID-ordered
pages of 32, reporting unreadable entries and unrecognized-file counts without deleting them.
Linux/Windows expose local import and paged-list commands. Both require the caller's viewed
identity, retain its lease through blocking work, and use the identity's sealing secret.
Import validates the bounded TAR before creating local state. The composer's manager offers TAR import,
a paged collection list, retry and text-only error details in English and Russian. Files above
8 MiB are refused before browser reading. Each opening owns its callbacks; closing or changing
identity discards pending display updates and prevents imports whose file read has not finished.
The core supports explicit removal of a selected saved PackID, including corrupt content,
without pruning other packs or interrupted files. Removal frees the counted file bytes and is
serialized with saving; it does not erase other copies. Desktop offers a per-row Remove action
with native confirmation, Cancel/Escape and a full PackID. Old dialogs and completions cannot
change another view or identity. The request/response flow below and sending remain planned.

"Add pack" will send tag 15 into the same chat. The request waits in the mailbox if the sender is
offline, so no peer has to be online. The device that sent a sticker from the pack into this chat
will answer with tag 16, at most once per peer and pack per day, reusing one pack blob. Requests for a
pack never sent into this chat are ignored, so a peer cannot probe what someone holds.
The Store now records per-peer/PackID sharing proof atomically with an own sticker. An incoming
sticker grants no permission. The proof survives local Clear/Delete and canonical group changes:
the peer already saw that pack ID. It is historical evidence only; response handling must also
check an open, unblocked chat and that the pack is still saved. The Store now queues authorized
response jobs with MLS/inbox/ACK, at most one generation per peer/PackID and 64 pending jobs
per identity. Due reads return at most 32 jobs. Failures use the outbox's 1 s exponential
backoff capped at 1 h; duplicate requests cannot reset it. Stored daily cooldown is checked
before queueing. Response completion now commits its exact outbox and the rolling 24 h
cooldown together, requiring the current job token, an open/unblocked shared pair and a
regular saved-pack file. Saved-file presence is rechecked under the same writer lock as
pack removal. Old workers cannot finish a newer job; missing packs can retire work without
advancing cooldown. Session receive routes controls through these guards. Allowed-chat controls
receive common receipts uniformly, including unsolicited/duplicate controls, then are consumed
before history/UI. Blocked and pre-match controls create no work. Generic Session sends refuse
control variants. Dedicated staging derives requests from a selected received MessageKey and
responses from a current job; Store projection, sealed MLS and exact writes commit together.
Failed local commits invalidate advanced in-memory MLS, so reopening restores the old snapshot.
Send wrappers deliver only after commit. `respond_sticker_pack_requests` now processes at
most four due jobs per call, honoring a stop callback between jobs and after upload. It checks
owner, partner/open/block/sharing state, loads a validated saved pack, reuses the typed ledger
and photo attach/put transport, and strips release capability before staging the answer.
Missing/invalid packs retire work without deleting files; transient reads/uploads keep a
scheduled retry. Reuse also verifies immutable plaintext and ciphertext binding. Corrupt ready
keys cannot stage a response or a new sticker history row. Tests cover wrong owner, stop/block,
removed packs and five due jobs leaving one for the next batch. Failed local
staging reopens committed MLS before further work. After successful staging, mailbox failures
belong to the common outbox. Desktop polling runs the bounded workers; receiver processing is described below.

The Store now stages up to 64 outstanding peer/PackID requests with MLS and exact outbox
writes atomically. Each request is derived from an existing received sticker in an open,
unblocked pair; it retains the selected hash and generation across restart and retry. A stale
save completion cannot retire a new request. Tests cover changed block/open state, wrong or
cleared selections, quota exhaustion, retry at capacity and simultaneous last-slot requests.
The Store now admits one canonical answer only into the matching pending peer/PackID row,
with MLS snapshot, inbox digest and ACK in one commit. Unsolicited, cross-peer, blocked and
closed-chat answers are ACKed without retaining a descriptor. Replays and competing answers
cannot replace the first descriptor; reads require the active request token. Corrupt stored
answers fail closed. Admitted answers now have durable download failure codes (fetch, invalid,
quota or storage) and the same 1 s..1 h backoff. Due reads expose at most 32 metadata-only jobs;
duplicate answers cannot reset the deadline. Explicit download retry keeps the descriptor and
clicked hash but rotates the token, so old callbacks cannot complete or defer the new attempt.
Waiting requests, failures and retries survive restart. Requested-pack saving rechecks token,
clicked hash, PackID and open/unblocked pair under the same writer lock as quota/publication.
The pending row retires in that commit, even when the validated file already exists. If rename
succeeds but the SQLite commit fails, intent remains and retry finishes against the existing
file; nothing is evicted. The core download worker now processes at most four due admitted
answers, reusing a validated local file when available. Otherwise it fetches through the photo
attachment path, validates the complete pack and clicked hash, and uses guarded publication.
Stop checks surround work and successful fetches; an in-flight transport retains its own deadline.
Blocked/closed pairs retire only current intent. Missing joined groups and fetch/validation/quota/
storage failures keep typed scheduled retries. Tests reject malformed/wrong-ID packs, missing
clicked hashes, unsolicited/stale completions and a block during fetch. A stop after transport
leaves intent unchanged; unreadable local files retain retryable storage status. Five admitted
local recoveries leave one for the next pass. Explicit fresh-answer staging now rotates the token,
clears the previous descriptor/failure and commits new tag-15 writes with MLS atomically. It keeps
the originally clicked hash even after local history was cleared; failure retains the old answer.
Old callbacks cannot change the new attempt. Ordinary download retry retains its useful descriptor.
Already queued requests and the sender's daily answer limit remain in effect. Desktop mailbox
passes now run both bounded workers on the configured node under the identity/session lease.
Completed saves contribute only to the local refresh count; controls remain absent from history
and unread notifications. The request UI remains planned.

The recipient accepts an answer only for a request it has pending in this chat. The pack must hash
to the requested ID and contain the sticker that was tapped. It is sealed into the identity's folder
like `originals/` and kept until the user deletes it. A saved pack equals an imported one: its owner
sends from it on their own quota and answers requests in chats they sent it into.

A pre-match chat follows the attachment rules in
[likes, matches and chats](chats-and-matches.md): a non-match's sticker is dropped before a reply,
and tags 14 and 15 from a non-match are refused.

## UI

- The composer gets a sticker button. Its panel has pack tabs and 20 local recents, and an empty
  state with "Add pack" that picks a TAR file.
- A sticker is sent alone, with no caption and no bubble, at most 192 pt.
- Delete works through tag 9. Edit and forward are hidden and refused by the codec.
- The chat list and notifications show emoji and "Sticker", including the closed-app paths
  (`MailNotice`, `MailJob`).
- The accessibility label is "Sticker: 😺, pack Cats".
- Settings lists packs with their size, and offers import and delete.
- Errors name the file and the reason. Imported filenames are quoted with escaped
  formatting/control characters so hostile text cannot reorder an error message.

## Lifecycle

- Packs, cache and recents belong to one device and are not synced between linked devices.
- They live in the identity's folder and go with it.
- Individual stickers follow the common [chat-media storage policy](client-local-storage.md),
  including auto-download, the shared off/256 MiB/1 GiB originals setting, LRU and cleanup of
  unreferenced media when history is cleared. They have no separate 50 MiB cache or sticker-only
  retention policy. Core reference tracking and sealed-file storage now share the photo path:
  restart, shared LRU/off settings and last-reference cleanup are covered with real files and
  validated first-frame pixels. Saved packs survive these operations. Desktop now exposes
  `chat_sticker_frame`, bound to viewed identity and current peer/group/author/message. It checks
  kept bytes, then a validated saved pack, and fetches only under the photo automatic policy or
  an explicit tap. Selection and transition guards run around I/O; retention uses the common cache.
  Its binary response is little-endian u32 width and height followed by RGBA8, at most 512×512;
  peer WebP and capabilities never reach the WebView. Canvas rendering remains planned.
- Deleting a pack leaves independently retained chat-media copies available under that policy;
  otherwise history shows the fallback. A deleted pack is no longer answered for.
- Local cleanup has the same [best-effort deletion](../decisions/0011-best-effort-deletion.md)
  limits as other chat media.

## Accepted leaks

Nodes see the same blob fetched by many readers and can correlate them. Reusing one blob saves
quota and traffic at that cost. Content stays sealed. A recipient can reshare a sticker's descriptor,
as they could any file they received.

## To measure

- Real sizes and decode CPU of 512 px, 3 s WebP against the 256 KiB limit.
- The 16 MiB decoder budget and sticker usage within the shared chat-media storage limit.
- Whether the client upload path shards as the node quota assumes: a 6 MiB pack is charged 10 MiB
  when sharded.
- Library growth from `image-webp` against the release size check.

## Cost

Rough sizes, not commitments: format and validator M, store and upload M, wire and FFI M, each of
Linux/Windows, Android and Apple UI M, end-to-end test S. Android and Apple depend on chat
attachments reaching their clients, which is not done yet and now effectively blocks v0.0.1 too.
