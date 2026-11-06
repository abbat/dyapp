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
Storage, chat protocol and UI integration remain planned. Decoder CPU,
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
| Received sticker cache | 50 MiB (*assumed*), least recently used goes first |
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

Three planned chat variants. All are MLS application messages, so nodes see only the sealed blobs.
The tag numbers below require reassignment before implementation: outer tag 13 already
belongs to private own-device `PeerChange`. Never decode sticker content under that tag.

| Tag | Body |
|-----|------|
| 13 sticker | 16-byte logical ID, HLC, pack ID, sticker SHA-256, emoji, pack name, optional attachment descriptor |
| 14 pack request | pack ID |
| 15 pack answer | pack ID, attachment descriptor |

Tag 13 leaves room for an optional thumbnail. No client release exists yet, so no older client
needs a stub.

Each descriptor sent to peers carries a zeroed `release_secret`. One blob serves many chats, so a
recipient must not be able to release it for the others; the sender keeps the real secret. The
codec already takes any 32 bytes there.

The pack blob holds a protobuf record of the manifest bytes and the files in manifest order.

## Flows

### Send

The first send of a sticker uploads it as its own attachment and waits for the upload, as a photo
does. The descriptor is stored and reused for every later send, in any chat, and the blob is
uploaded again when under a day of the 168 h retention is left. Node quota charges small blobs five
times, so a 256 KiB sticker costs about 1.25 MiB of the 64 MiB owner quota. Past the sender's budget
a sticker goes without a descriptor and the peer sees the fallback.

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
Import validates the bounded TAR before creating local state. Settings now offers TAR import,
a paged collection list, retry and text-only error details in English and Russian. Files above
8 MiB are refused before browser reading. Each opening owns its callbacks; closing or changing
identity discards pending display updates and prevents imports whose file read has not finished.
The core supports explicit removal of a selected saved PackID, including corrupt content,
without pruning other packs or interrupted files. Removal frees the counted file bytes and is
serialized with saving; it does not erase other copies. Desktop offers a per-row Remove action
with native confirmation, Cancel/Escape and a full PackID. Old dialogs and completions cannot
change another view or identity. The request/response flow below and sending remain planned.

"Add pack" sends tag 14 into the same chat. The request waits in the mailbox if the sender is
offline, so no peer has to be online. The device that sent a sticker from the pack into this chat
answers with tag 15, at most once per peer and pack per day, reusing one pack blob. Requests for a
pack never sent into this chat are ignored, so a peer cannot probe what someone holds.

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
- Deleting a pack keeps stickers in history visible while the cache holds them, then the fallback
  shows. A deleted pack is no longer answered for.
- Sticker bytes stay in the cache after a message is deleted, until evicted; this is covered by
  [best-effort deletion](../decisions/0011-best-effort-deletion.md).

## Accepted leaks

Nodes see the same blob fetched by many readers and can correlate them. Reusing one blob saves
quota and traffic at that cost. Content stays sealed. A recipient can reshare a sticker's descriptor,
as they could any file they received.

## To measure

- Real sizes and decode CPU of 512 px, 3 s WebP against the 256 KiB limit.
- The 16 MiB budget and 50 MiB cache.
- Whether the client upload path shards as the node quota assumes: a 6 MiB pack is charged 10 MiB
  when sharded.
- Library growth from `image-webp` against the release size check.

## Cost

Rough sizes, not commitments: format and validator M, store and upload M, wire and FFI M, each of
Linux/Windows, Android and Apple UI M, end-to-end test S. Android and Apple depend on chat
attachments reaching their clients, which is not done yet and now effectively blocks v0.0.1 too.
