# Profile photos

How a client publishes, fetches and keeps public profile photos. The wire format is in
[protobuf-schema.md](../architecture/protobuf-schema.md); node rules are in
[bootstrap.md](../architecture/bootstrap.md#signed-profile). `rust/client` implements storing and
publishing, and the desktop has the editor and shows photos on search and liked cards, chat rows
and the open identity's avatars and in `profile-view`, and its catch-up pass runs the keep refresh
of step 4. The FFI exposes the draft photo commands, runs the keep refresh in
`ChatSession::receive`, and fetches other people's photos into `ProfileSearcher`'s memory cache.
The Android profile editor adds, moves and removes photos (`ProfilePhotos.kt`, `PhotoPrep.kt`),
and Android shows other people's photos on search results, liked cards and the profile page
(`PeerPhotos.kt`) and the draft's main photo on the own avatar. Android chat rows and the chat
header show the peer's main photo with the contact name (`ContactNames.kt`), from profiles the
searcher fetches when Chats or a chat opens. The Apple profile form adds, moves and removes photos
(`ProfilePhotos.swift`, `PhotoPrep.swift`), and Apple shows other people's photos on search results,
liked cards, chat rows, the chat header and the profile page (`PeerPhotos.swift`). Apple has no own
avatar to show. Android reports network capabilities to the core, treating missing or unknown
capabilities as metered. Apple reports expensive or constrained paths through `NWPathMonitor`.
Until a platform reports its network, the core conservatively treats it as metered.

## Limits

| Limit | Value | Why |
|-------|-------|-----|
| Photos a client publishes | 6 | The schema allows 16; six fit the quota below with room for chats |
| Full image | One JPEG blob, at most 1 MiB, long side 2048 px, then 1600 px, then 1280 px | Enough for a full-screen view; stays a whole-copy blob |
| Thumbnail | One JPEG blob, at most 24 KiB, long side at most 320 px | The existing `Photo.thumbnail` rule |
| Picked file | JPEG or PNG, at most 64 MiB and 64 megapixels | The bounds chat photos already use |

The node bills a whole-copy blob at five times its size, so a photo at the limit costs 5.1 MiB and
six cost 30.7 MiB. `limits.media_per_owner_mb` therefore defaults to 64: chat attachments share
the quota, and one sharded 6 MiB chat photo costs about 9 MiB. An operator may set less; the client
then shows the node's `FULL` reply and keeps the photo local.

## Shared preparation core

`dyapp_client::photo::prepare` supplies header-driven decode plans and a bounded RGBA-to-JPEG
pipeline. Profile originals use the limits above and qualities 85, 70, 55; chat originals use
6,000,000 bytes, sides 2560, 1920, 1280 and quality 85. Both share preview encoding and flatten
transparency onto white. The platform must decode upright straight RGBA within the returned
size before calling preparation. Android uses this common API for profile and chat picking,
including decoder targets for displaying originals. iOS and desktop preparation migration
remains planned.

## Publishing

1. The page decodes the picked file and draws it on a canvas, which writes a bare JPEG. EXIF, GPS,
   colour profiles and anything appended to the file never leave the computer, and a rotated
   photo comes out upright. Android uses software `ImageDecoder` with the core's header-driven
   target, then converts `Bitmap.getPixels` straight sRGB ARGB into RGBA. The shared Rust pipeline
   flattens transparency onto white and encodes both original and preview. Core dimensions travel
   with the prepared bytes to chat sending; Kotlin does not parse the JPEG again. Bitmaps are
   recycled after preparation. The system photo picker needs no storage permission.
   Apple clients do this with ImageIO, then pass the JPEG through the core's `strip_metadata`,
   because ImageIO may write EXIF of its own; the sRGB profile stays, as it names no device. iOS
   uses the system photo picker and macOS a file dialog.
2. The core refuses anything but a JPEG within the size and pixel limits. It keeps both blobs in
   the local store under their SHA-256 hashes and lists them in the profile draft. Addition commits
   both blobs and the latest draft in one SQLite immediate transaction; failure leaves them unchanged.
   Removal and reordering also acquire writer ownership before reading and transforming the draft,
   preserving other fields committed by independent handles.
3. Publishing sends `keep` with every hash of the draft, puts each blob the node reports missing,
   then publishes the profile. A failure before the profile publish leaves the previous profile
   valid. A photo removed from the draft leaves the store and the keep list with that publish, so
   a reader of the older profile briefly sees the placeholder. Withdrawing the identity sends an
   empty keep after the tombstone.
4. The catch-up pass refreshes the keep with a newer signed `time`, at most once a day, and puts
   back whatever it reports missing, so photos outlive node churn while the owner is active. The
   list comes from the node's live profile, not the draft, whose unpublished removals would
   release a photo readers still see. A failed attempt waits for the next day.

`keep` uses the frozen profile `version`. On `STALE`, the client never raises that version with
an earlier hash list: doing so could release a newer publication's photos. Instead it repairs
only through `put`, which checks current owner authorization without replacing the keep list.
Missing local blobs are fetched by verified hash and checked against the profile blob bounds before
repair. A same-list/same-signed-time failed attempt can therefore retry without a clock wait.
This fallback may transfer already stored bytes; a removed hash fails rather than replacing newer
references. A keep-refresh reports `STALE` when only bytes were repaired: retention was not renewed.
Recovery from an abandoned conflicting higher-version staged keep remains planned, including
changed-draft publication and live-profile retention renewal; byte repair alone does not solve it.
The store holds own photos until a publish drops them, so ordinary repair needs no original file.
The bare JPEG and byte limits are checked where the blob enters the store; the desktop and
Android check full-image pixels before that; full-source decoding remains a platform service,
while the shared core bounds and decodes previews. On Android and Apple
draft writes still hold a process-local lock around proof/publication and their other writers.
Shared core transactions now serialize editable saves and photo changes across database handles;
proof solvers now merge only proof/cursor progress into the latest draft after computing outside
writer ownership. Pending cursors do not replace a valid lower-difficulty proof. Tauri editor saves
now apply validated fields/interests inside that same latest-draft transaction. Own publication
captures a baseline before network work and overlays later local photo/field edits at completion.
Foreign/import paths and pending-outcome adapters still require adoption before frontend locks go.

Blob cleanup itself holds an immediate SQLite writer transaction while reading the latest draft
and pruning. It retains the union of the supplied published hashes and current draft hashes.
Invalid references, noncanonical blob keys or unsupported blob formats refuse cleanup; any failure
rolls back the entire batch. Key selection uses the exact case-sensitive `profile_photo:` namespace,
not SQL `LIKE` wildcard matching. Own-publication completion now includes this cleanup in the
same transaction as the merged current draft, search queue and visibility. Current photo order and
bytes survive edits during network work, while exact published references also remain protected.
A cleanup failure rolls back all those local projections. Foreign/import projection is still separate.
Local hide already reads the latest draft and atomically commits its new version/search/hidden state,
preserving photos and other concurrent local edits instead of copying the caller's old snapshot.

Android removal and reordering carry the displayed snapshot's opaque core revision. It covers all
original and preview hashes, their order and duplicate entries; unrelated fields/proof do not change
it. Core verification and mutation share the writer transaction. A stale action returns a typed
conflict, reloads the list and asks the user to review it; the old index is never retried automatically.
Images and revision publish together, and removal confirmation retains the revision from its opening.
Apple and Tauri adoption of this checked contract remains planned; their legacy index-only UI paths
still require migration. Swapping identical duplicate entries is equivalent; equal previews of
different originals are not.

## Viewing

Lists and results fetch the first thumbnail; `profile-view` fetches all thumbnails and full
images. `get_media` checks each blob against its hash, and the desktop commands apply the size and
pixel bounds before the page decodes it. One command fills the cache over one set of connections
and another reads a blob from it, so a page of cards connects once.
The page draws to a canvas, because the content security policy blocks `data:` and `blob:` images.
Android decodes thumbnails with the core's `check_preview`, so a crafted JPEG never reaches the
platform decoder, and full images with `ImageDecoder` at most 2048 px on the long side; the
shell's search session owns the cache, so the search and every page share it.

Other people's photos stay in memory, in a least-recently-used cache of 256 thumbnails and 8 full
images: at most about 10 MiB, nothing about whom the user looked at on disk, and no eviction job.
The cost is a refetch after a restart. A change of identity or network empties it. Under
Settings → Photos, a metered or unknown network shows thumbnails and loads a full image on a tap,
as chat originals do.

## States

| State | What shows |
|-------|------------|
| No photos | The avatar letters, and Add photo in the editor |
| Six photos | Add photo is disabled and says why |
| Wrong type, too large, cannot be read | The status line names the problem; nothing changes |
| Node refused or unreachable | The status line says so and a full node gets its own text; the photo stays local for a retry |
| Blob missing, fails its checks, or offline | The avatar letters on that photo; a later view tries again |
| Removing a photo | A confirmation first |

Order changes with Move left and Move right, and the first photo is the main one, so every step
works from the keyboard. Drag is not planned.

## Abuse

Upload limits are the quota above, the per-peer request and byte limits, and the proof of work on
the profile. An operator refuses a named blob hash or owner key
([abuse and legal requests](../operations/abuse-and-legal-requests.md)). No scanning runs on the
client or the node. Automated tests cover a non-JPEG, an oversized blob, a blob of another hash and
a JPEG that claims more pixels than the bound.
