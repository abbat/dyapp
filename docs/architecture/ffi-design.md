# FFI Contract

`ChatSession.media_cache`, `set_media_cache(enabled, limit_bytes)` and
`clear_media_cache` expose the device-local encrypted originals cache. The record contains
only enabled state, remembered byte budget and used bytes. Calls hold the same session
mutex as media retrieval, use no network and preserve history and saved sticker packs.
Desktop exposes the same three commands with the viewed identity ID and a lease retained
through background filesystem work. A fetch already in flight may retain a new copy after
a desktop clear; the returned usage is measured from disk.

> ⚠️ **Prototype.** This page is the single source of truth for the Rust ↔ native
> boundary, checked against `rust/ffi/src/lib.rs` (UniFFI 0.32.2). Kotlin bindings are
> generated during the Android build, Swift bindings by `tools/scripts/build-apple-core.sh`
> ([FFI bindings](../development/ffi-bindings.md)).

## Ownership of shared logic

[ADR 0002](../decisions/0002-rust-core-native-apps-tauri-desktop.md) places validation,
parsing, policy, state machines and storage rules in shared Rust. Expose these through UniFFI
before adding their Android or iOS UI; desktop commands use the same core APIs. Kotlin and
Swift retain rendering, input, navigation and required platform services. A client-side
exception requires a concrete reason recorded in the task and beside the implementation,
with maintainer agreement before implementation. Moving code preserves validation, errors
and security invariants; test the shared rules independently of platform callback timing.

## Network attempt ownership

`ConnectionAttempt(checker)` prepares a single-use cancellation owner without key access,
admission or network work. Its shared-core `Cancellation` retains cancellation before
subscription. A native run checks before and after identity admission and prioritizes
cancellation over a simultaneously ready seed result. Cancellation cannot release an
in-flight identity lease; the actual worker must finish first. Retries allocate new attempts.
The legacy `ConnectionCheck.run/cancel` interface remains available; clients must adopt
attempt ownership to cover cancellation before dispatch. Android uses an attempt per check;
its presentation rechecks cancellation before publishing an IO result and closes the attempt
on non-cancellable IO after actual completion, including disposal.

## Shared mobile identity registry

`MobileIdentityRegistry` exposes core inventory, resolution, candidate inspection, explicit
creation, verified selection and confirmed removal for Android and iOS wrapped-key layouts. `MobileSlotKeys` callbacks implement only
credential presence, `open_wrapped`, `wrap` and `erase` by slot ID, reusing opaque `IdentityKey` objects
and the existing `KeyStore` interface. No additional foreign plaintext-key API is introduced.
Rust owns file access, lifecycle markers, reservation, locking and recovery classification.
The core inventory also recognizes old Android/desktop `.<uuid>.removed` directories as
interrupted deletion, even when cleanup removed their markers. A visible/hidden duplicate ID
is damaged metadata; neither copy opens as ready. Reservation skips IDs held by hidden
cleanup paths. iOS retains its separate `<uuid>.deleting` marker format; a tombstone never
makes symlinked paths or malformed state markers readable. A regular directory whose state
file was already cleaned remains deleting under its valid tombstone. Inventory never
cleans these paths; confirmed cleanup is a separate explicit core mutation.

`MobileSlotResolution` preserves setup, picker, ready and recovery outcomes, including
missing versus unavailable credentials and recoverable Peer IDs. Creation and selection return
`MobileSlotSelection`: its key is verified and its selection committed; a nonempty
`durability_detail` reports unconfirmed directory persistence after that commit, rather than
a failed selection. Construction does not create an identity. Native `remove` returns an
optional persistence diagnostic only after visible completion; after a failure, resolve again
to show interrupted cleanup. Android uses this registry; migration of the Swift slot adapter remains planned.

`Registry::operation` admits only a listed ready identity whose actual key matches the expected
Peer ID. Its `IdentityOperation` retains a shared OS lock for the whole operation; create/select
and confirmed removal use the exclusive lock. Inventory and resolution also use shared
locks, so they can run alongside admitted operations. Every acquisition opens its own handle;
never upgrade a held lease to a mutation lock. The common native `MobileIdentityOperation`
exposes an opaque key and verified slot; retaining the key alone does not retain the lease.
Move the operation object into blocking workers and keep it until actual completion, including
after UI cancellation. Its `remove_saved_sticker_pack(pack_id)` delegates one explicitly
confirmed selection to core storage without opening chat keys or requiring a media sealing
secret; corrupt saved content remains removable. Missing files return false. Invalid PackIDs
refuse before storage work. An error after unlink does not prove rollback: refresh the collection
before deciding whether to retry. The operation retains its identity lease through completion;
other packs, history and outbox are untouched. Installation-owned node work must run outside identity leases.
Android's `NativeSlotKeys` implements the common credential callback with existing alias names
and typed errors, using opaque keys. Its callback inputs release their native key handles after
wrapping. Android's `Slots` now delegates lifecycle operations to this registry; Swift adoption
remains planned.

`Registry::remove` takes the same exclusive lock. It rejects legacy/damaged candidates,
checks an existing key against the exact confirmed Peer ID and validates all owned files
before erasing credentials. Confirmed absent mobile credentials permit cleanup without
decoding an orphan wrapped blob. Access failures never count as absence. Platform callbacks
address typed credential roles only; core erases/verifies seal and device credentials before
identity credentials. Mobile wrapping credentials protect all of that slot's local key blobs.

Only known client files and flat `originals`/`sticker-packs` contents are accepted.
Media names are lowercase 64-digit hashes; sticker pending files use a dot, 32 hex digits
and `.pending`. Unknown paths, symlinks and identity-local node files stop removal before
any credential erasure. Installation-owned node and app-lock paths are outside cleanup.

Removal marks deletion and clears only the target's active selection before erasure.
Android/desktop cleanup moves to the existing hidden format; iOS retains its sibling tombstone
and explicit `none\n` selection. Cleanup revalidates paths and keeps the empty directory until
the final removal, so intermediate failures remain discoverable. The iOS tombstone goes last.
`RemovalOutcome.durability_detail` distinguishes visibly completed deletion from unconfirmed
directory persistence; it never reports a post-commit sync failure as an uncommitted removal.
Android removal delegates to this API; the Swift adapter still needs migration.

`Registry::chat_operation` bootstraps wrapped mobile `device.key` and `chat-state.key`
under the exclusive registry lock. It validates existing keys before creating any missing key,
checks a stored MLS snapshot against the existing state/device keys, and refuses to regenerate
keys for initialized chats. Fresh wrapped bytes are verified before and after exclusive,
synchronized file creation. Partial bootstrap preserves the first verified file for retry.
Recovery confirmation uses `Registry::existing_chat_operation` instead: it requires an initialized
database and verifies the existing owner, device and sealing keys without bootstrap or replacement.
`Store::open_existing` disables SQLite file creation and schema initialization, including for
an existing empty file. The returned operation retains the same registry/session locks only
through actual work; a review dialog retains neither operation nor session.

Bootstrap releases its exclusive lock before shared identity admission; it never upgrades a
held shared lease. The returned `ChatOperation` retains both identity protection and an
identity-local exclusive `chat.lock` for actual session completion. Inventory may still read
alongside it. The lock file belongs to the identity and confirmed cleanup accepts it.
`MobileIdentityRegistry.open_chats(id, expected_peer, store)` delegates to this bootstrap and
retains its guard inside the returned `ChatSession` until the last session owner completes.
The supplied existing `KeyStore` must target the slot's existing wrapping alias and refuse
missing credentials. Before any bootstrap writes, core opens the saved owner blob through
that capability and compares it with the verified identity, rejecting a mismatched store.
The legacy explicit-key constructor remains for existing callers; those callers do not
gain registry protection until migrated. Native commands, construction and regression tests
live in separate `chats` modules without changing the existing command signatures.
Android `RustBridge.withChats` now uses this API directly with an existing-only store; Kotlin no
longer generates or persists ancillary chat keys. Apple adoption remains planned; desktop
system/file bootstrap continues through the existing desktop implementation.

`ProfileSolve.for_slot(registry, id, expected_peer)` binds an idle proof handle to a verified
identity without retaining a lock between steps. Every actual step reacquires the core operation
before accessing its database and retains it through completion, including cancellation. A removed
or replaced identity fails admission before storage access; retaining an idle handle cannot prevent
selection or removal. The internal native admission adapter stores only coordinates and delegates
all lifecycle/key checks to core `Registry.operation`. The explicit directory/key constructor is
still unguarded for existing callers. Android proof steps use the bound factory; Apple adoption
remains planned.

`ConnectionCheck.for_slot` uses the same per-call admission for saving/checking a draft and
activating a successful check. A retained checked result cannot write into a removed or replaced
identity; an idle checker holds no registry lock. Cancellation retains protection until the current
run actually returns. These checks only use identity-owned network settings and never start, stop
or configure the installation node. Android uses the bound checker; its pre-registration
cancellation ownership and Apple checker adoption remain separate work.

`ProfileSearcher.for_slot` binds searches, contact-name reads and network profile/photo fetches
without retaining a lease while idle. `search_slot` and `contact_names_slot` obtain the verified
key inside the admitted call, so callers must not wrap them in another identity lease. Existing
explicit-key methods reject a key from another identity on a bound worker before database access.
All bound-worker disk/network calls admit themselves; callers must release external identity leases
before invoking them. In particular, Android search and contact-name calls must switch from their
old `withKey` wrappers to these keyless methods when adopting the bound factory.
Pure cached-photo reads and requests already satisfied in memory do not acquire a registry lease;
network work reacquires protection before reading settings. A stale worker cannot recreate removed
data or access a replacement identity. Android search and contact-name reads now use these bound
APIs; Apple adoption remains planned. Android's remaining composition-time factory and data reads
still require IO dispatch with cancellation-safe resource ownership.

## Which source is the contract

| Source | Status |
|--------|--------|
| `rust/ffi/src/lib.rs` + `rust/ffi/src/dyapp.udl` (crate `dyapp-ffi`) | **The contract.** Workspace member; compiles to `libdyapp_ffi.{so,a}`. Interface is declared with proc macros; the UDL only supplies an empty `namespace dyapp {}` for the UDL scaffolding. |
| `clients/android/app/src/main/kotlin/com/dyapp/RustBridge.kt` | Retains verified native operation/session ownership and projects identity action results. |
| `rust/ffi/src/android.rs` | Android only: the JNI export `DYApplication.nativeInit` that gives the DNS resolver the app `Context` ([ADR 0032](../decisions/0032-android-context-for-dns.md)). |
| `clients/android/app/src/main/kotlin/com/dyapp/Slots.kt` | Thin common-registry adapter for identity slots under `noBackupFilesDir`: create, open, recovery and removal; see [Android identity storage](../development/android-identity-storage.md). |
| `clients/android/app/src/main/kotlin/com/dyapp/PlatformKeyStore.kt` | The app's `KeyStore`: AES-256-GCM with a non-exportable Android Keystore key, one alias per identity slot. |
| `clients/ios/DYApp/RustBridge.swift` | Swift bridge for stored opaque keys and the iOS profile/network flows. |
| `clients/ios/DYApp/PlatformKeyStore.swift` | The Swift `KeyStore` adapter: CryptoKit AES-256-GCM with a random key in the Keychain, this device only. |

The Android, iOS and macOS apps link the library. Linux and Windows call
`dyapp-identity` directly; their [storage choice and capability checks](../development/desktop-identity-storage.md)
are implemented in the Tauri shell, outside this UniFFI contract.

## Current desktop runtime ownership

The Tauri shell stores the selected identity and slot directory in one admission gate.
Identity-bound network, profile, search, privacy, chat, safety, clipboard, node and usage
commands hold a gate lease through their blocking, async or synchronous work. A blocking worker
owns its lease if the outer command is cancelled. The client `Store` and `Session` are opened
for a command and dropped when it returns; `chats::Sessions` serializes MLS operations with each
other. The process also holds one `SearchState` profile cache and the current profile solve's
per-attempt cancellation token.

Identity selection closes admission, signals the current profile solve and seed check to cancel,
and drains held leases off the UI thread, then stops the embedded node and opens the target slot
before changing the persistent selection. Startup, creation and removal also commit through the
gate. A failed creation or removal reopens the persisted selection or previous slot; it admits
no identity if that slot cannot reopen. Once old leases drain, every identity transition clears
the shared verified profile cache and remembered successful seed check. A solve or seed check that
registers after the transition signal probes the gate and cancels itself. The UI asks the node to
apply the new identity after rendering it. Network activation consumes the successful seed check;
a late check cannot restore it. Saving different active settings also clears verified profiles
and advances the cache epoch under one lock, so late search and chat results cannot repopulate it.
Repeating the same settings retains the cache. UI callback guards are specified in the
[desktop runtime lifecycle](../design/desktop-runtime-lifecycle.md).

## Shared photo codecs

`dyapp_client::photo` exposes preview encoding, bounded peer JPEG decoding and frame inspection
directly to Rust clients, with no UniFFI dependency. It retains the current 320 px / 24 KiB
preview policy and 1 MiB decoder allocation cap, and rejects overflowing raw pixel dimensions.
The common UniFFI preview adapter delegates to this core and only converts pixel records and
errors. Its Kotlin/Swift API stays unchanged; Tauri can call the same core functions directly.
`dyapp_client::photo::prepare` now owns typed profile/chat purposes, source limits, decode
plans and full-photo fitting/quality/byte decisions. It accepts upright straight RGBA within
the returned purpose target, flattens once, and emits an original plus the shared preview.
Source headers are bounded before decoder allocation; malformed pixels and oversize inputs
fail with typed errors. The common FFI exports `PhotoPurpose`, `photo_picked_byte_limit`,
`photo_decode_plan` and `prepare_photo_pixels`, with `PhotoDecodePlan`, `PreparedPhotoData`
and typed `PhotoPreparationError` conversions. Kotlin/Swift use this one surface; adapters
contain no fitting/encoding policy. Android's picker and original display use core decode
plans; its software bitmap pixels enter the same core encoder for both profile and chat.
The Tauri profile and chat commands call the same core functions on page-decoded RGBA.
The iOS pipeline switch remains planned.
`validate_original` checks the selected purpose's encoded byte limit, JPEG framing, the
64-million-pixel cap and descriptor/frame dimension agreement before native decoding.
Mobile chat send and both cached/network original retrieval use the chat budget of
6,000,000 bytes; profile validation retains its separate 1 MiB limit. Framing checks do
not replace the platform decoder's validation of compressed pixel data.

Android draft loading reads only the identity-bound `client.db` through `load_profile_draft`.
An absent draft starts empty; key/database errors propagate. Legacy JSON is neither imported
nor modified. Publication and mutual search use the same loader.

## Shared chat link destinations

`dyapp_client::chat_links::hosts` extracts explicit HTTP(S) destinations in first-occurrence
order, deduplicated by canonical ASCII host and nondefault port. It retains desktop token
boundaries, ECMAScript whitespace and trailing prose punctuation. WHATWG URL parsing resolves
userinfo, IDNs and IP addresses; credentials and paths never become the displayed host.
Each result carries mandatory ASCII/Punycode and, for an IDN, an additional Unicode spelling.
Unicode must not replace ASCII in destination display. The function performs no DNS, preview
fetch or opening action. Shared input whitespace lives in `dyapp_profile::text_input`.
The common `chat_link_hosts` adapter only converts these results to `ChatLinkHost` records.
Desktop message projections use the core ASCII destinations as `linkHosts` in history and list
records, recomputed from current live text and empty for deleted/media entries. JavaScript only
renders the supplied captions, without URL parsing or extra IPC. Android/iOS host-line presentation
remains planned.

## Shared chat composer policy

`dyapp_client::chat_composer::assess` projects conversation phase and text into writable,
too-long and sendable controls. Loading, a pending outgoing request and a closed chat disable
submission. The shared protocol limit is 65,536 UTF-8 bytes, inclusive. Blank text follows the
Linux desktop's ECMAScript trim semantics, including BOM as blank and excluding NEL from blank.
The common `chat_composer_policy` native adapter converts states/results; Android delegates its
composer helpers to it. This presentation policy does not replace send authorization. Android
text/edit/delete and photo actions consume the committed submission APIs below; projection reads
refresh the UI and never determine whether an operation committed.
The session staging path separates preflight context, MLS preparation and atomic persistence;
failures after MLS advancement invalidate the in-memory device/groups. Delivery retains token
fallback replacements across ranked nodes, propagates local failures immediately, and records
node acceptance separately from peer receipts.
`Session::submit_text_to`, `submit_edit` and `submit_delete` return a committed `Submission`:
`Accepted` or `Queued`, both with the logical message/operation ID; queued also retains the
immediate delivery error. Profile/key-package/staging errors remain errors before successful
commitment. Existing strict send methods use the same implementation and require node acceptance
via `Submission::into_accepted`. Forwarded text and sticker metadata have matching
`submit_forwarded_text(_to)` and `submit_sticker(_to)` methods with the same result and existing
open-chat/blocked guards. `submit_attachment(_to)` uploads the blob before message commitment;
`submit_pack_sticker_to` also retains upload reservations/reuse and strips the owner's release
capability from the chat descriptor. Both expose queued post-commit failures. The existing
saved-pack send API still returns an ID on committed client delivery errors and propagates other
failures. For native text submission, a missing active network or failed connection can use an established
open local pair: core checks the canonical target, block/conversation policy and existing MLS group,
then commits through the same durable staging path. Own-device listing admission remains required.
Only an absent local group may fall through to online first-contact admission; a staging refusal never
triggers another submission. The connected path still verifies the peer profile. Offline operation
cannot discover remote revocations it has not received.

Common native `submit_text`, `submit_edit_text`, `submit_delete_text` and
`submit_chat_photo` and `submit_saved_sticker` return `ChatSubmission::Accepted` or `Queued`
with the committed ID;
pre-commit failures remain errors. The adapter only converts core outcomes and operation-specific
error codes. Legacy native send/edit/delete methods keep strict delivery errors through the same
path. Android transports the native result through its identity lease: Accepted/Queued clear the
submitted text/photo draft, while a pre-commit error keeps it. A stale edit target still refuses
submission instead of falling back to a new message. Photo drafts never infer commitment from
localized status text. Profile-link sharing uses the same typed text result; its initial history
read checks open-chat eligibility only. Its dialog records completion from the native outcome,
independently of localized status details. Common native saved-sticker submission authenticates
an identity-local pack through core storage, then delegates upload reservations, descriptor reuse,
open-chat policy and durable submission to `Session::submit_pack_sticker_to`. Selection bounds
are checked before networking; no encryption key or release capability crosses the boundary.
Common `saved_sticker_packs` projects the core's bounded collection page, exclusive cursor,
authenticated name/count or explicit unreadable state, all-file byte accounting and unrecognized
file count. Unreadable entries retain no trusted pack label and remain saved. The read-only API
uses the same identity operation and session mutex, with no network, file removal or compressed
media returned. `saved_sticker_frame` authenticates one selected saved entry and returns its
manifest identity/index, label/hash and actual bounded first-frame RGBA from the core decoder.
Mobile code receives no WebP input to decode. `sticker_import_byte_limit` exposes the core TAR
bound for OS stream reads. Common `import_sticker_pack` uses the same core TAR/manifest/frame
validation and sealed storage calls as desktop; successful results retain the validated PackID
and whether it was newly saved or already readable. A persistence error may follow file publication:
refresh the collection rather than assume rollback or automatically retry. Invalid archives are
rejected before collection storage work; no chat mail or network publication is staged. Android
forwarding/sticker presentation and native CI verification remain planned.

Shared `sticker_media::select` binds the complete stored message key to its identity/conversation.
Its snapshot reads authenticated local originals/saved packs, applies automatic-download policy
using the adapter's OS network report, and revalidates current metadata before decoding/cache
retention. Cache-write failure does not discard validated pixels. Desktop and common native
`chat_sticker_frame` use this boundary. The native call retains session/identity ownership through
blocking completion; UI cancellation still requires the caller's presentation generation guard.
Network connection, desktop cancellation checks between async stages and binary/record projection
remain in the adapter.

Shared `sticker_picker::page`/`thumbnail` produce at most eight 96px RGBA choices in manifest
order, with authoritative bounds and labels. Desktop only projects their existing JSON fields.
Shared `sticker_picker::recents::page` owns hash-plus-emoji lookup, one-pack caching, missing/invalid
pack filtering and original-list cursors; desktop only projects the resulting flattened entries.
Callers retain identity admission and supply their transition signal. Cancellation and access/decode
failures never publish a partial page. Native picker/recent exports remain planned; collection
listings and individual full-frame reads are separate APIs.

Common native `receive` also runs bounded core sticker-pack responses and admitted downloads
after applying the mailbox, retaining the same session/identity operation through completion.
Only explicitly persisted download intents are processed. Completed saves add to the returned
refresh count; control messages stay hidden. Shared `sticker_requests::status` owns received-pack
state, authenticated clicked-hash checks and already-saved intent reconciliation. Desktop maps
these typed states to its existing JSON; an unreadable pack retains failure plus generation and
is never silently new. Status can write reconciliation state and needs guarded mutable storage.
Common `chat_sticker_pack_status` projects that core result as typed phase/failure,
optional 16-byte generation and exact retry deadline under the native session lease. It performs
no networking and never treats a lookup error as New or Saved.
Shared `sticker_requests::prepare` owns explicit action and generation admission: Retry/StopWaiting
complete using storage only; Request/Fresh produce an owner-bound plan after local publication
admission. Staging revalidates selection and the session's local device listing before the core
MLS/outbox commit. Desktop opens MLS only for such a plan; native callers can reuse their admitted
session. Common `change_sticker_pack_request` lifts typed actions and 16-byte generations into this
path while holding the existing session/identity lease. Retry retains its descriptor but rotates the
generation. `StickerPackSaveAttempt` separates local completion/no-op from subsequent status: Ready
carries status; Unavailable carries a local read failure and requires refresh, never blind resubmission.
Errors before completion remain errors; failed MLS staging can invalidate the session. No network is
contacted, and success never means node acceptance. Mobile Save pack UI remains planned.

`Session::submit_interaction(_with)` separately returns an optional committed
`InteractionSubmission`: the actual protocol event plus its node delivery result. `None` retains
the no-event meaning for a suppressed daily view or a non-like without a joined group. Errors
before interaction commitment remain errors, including a failed first-like invitation; a
separately staged invitation may already exist. Strict interaction methods share the same path.
The common native `submit_like` returns `ChatLikeAttempt`: an optional `ChatSubmission` keyed
by the real event ID, and an independent `ChatLikeState::Ready` or `Unavailable` projection.
A state-read failure never erases commitment; absent submission explicitly means no event.
Legacy `like` preserves delivery-error precedence over projection errors through the same path.
Android uses this result directly: only queued native submissions show pending delivery;
matching projection values never prove that an attempt committed. An unavailable projection
clears the shown like state and disables like actions until a new successful read. The profile
form belongs to the peer/identity; local operations version background reads, and completing an
operation restarts the read effect. Older read/operation completions cannot replace a newer
intent. Native Android CI verification remains pending.

`ProfileLoad.kt` owns the displayed profile state by peer, feed/identity and matching verified
known snapshot. A different owner recreates remembered state immediately; retries restart only
the current fetch and preserve an already shown profile. Blocking bridge calls stay on IO.
A fetch completion must still belong to an active producer before it can publish a value.

## Shared profile editor policy

`dyapp_client::profile_editor` validates editable fields with stable draft errors and separately
applies normalized fields to an existing `dyapp_profile::Profile`. It trims name/about, clears
seeking fields for not-looking, and preserves proof progress, photos, devices and all other
noneditable fields. Editor categorical/seeking codes use the closed vocabulary; node verification
retains its separate forward-compatible contract. Apply alone supports eligibility and search
projections; persistence callers must validate first. `profile_editor::normalize_interests` owns shared
interest-list limits and NFC/trim/lowercase normalization; Tauri and native saves use it. Native
`PublicProfileDraft.interests` is an optional edit: None preserves stored tags, Some(empty) clears,
and loaded drafts return Some with the actual tags. This keeps unrelated edits by older callers from
erasing interests. Mobile interest controls remain planned behind the UI review gate.
The common native draft adapter converts
records and errors, validates through this API before opening storage, and delegates save and
eligibility/search projections to the same normalized application. Editable saves use the core
`Store::update_profile_draft`: a SQLite immediate transaction acquires writer ownership before
reading the latest draft and retains it through transformation and commit. Refusal rolls back;
independent connections/processes cannot interleave another database write in that transformation.
Callbacks are short and synchronous, with no proof search, network or nested store writes.
Photo add/remove/move use the same transaction; adding commits both blobs and the draft
together, rolling all three back on failure. Native and desktop proof solvers use
`Store::prepare_profile_work` outside writer ownership and `Store::commit_profile_work` for a short
atomic merge into the latest draft. Per-difficulty cursors stay separate from valid proof nonces;
late pending/weaker work cannot downgrade a stronger proof or replace fields/photos/devices.
The prepared value is owner-bound, and cursor/draft commit failures roll back together. Tauri editor
saves also apply their validated fields/interests inside the same latest-draft transaction. Own
publication now captures `ProfilePublication` draft/visibility before the first network await in
foreground native and Tauri publishers. Device enrichment changes only its request, never its
stored baseline. Verified readback commits through one immediate transaction: post-prepare local
value changes win per scalar or coupled location/income/birth/seeking/photo/device-directory group,
stronger proof survives, and draft/search/visibility/blob retention commit or roll back together.
The exhaustive profile constructor makes new wire fields require an explicit overlay decision.
`ProfilePublicationOutcome` separates exact published values from the current draft and pending flag;
UniFFI's existing published payload remains exact remote output, alongside a fresh local snapshot.
Android applies that snapshot to its existing form without replacing unsaved or in-flight save inputs.
New pending presentation and remaining adapters still require migration before frontend monitors
can be removed.
Legacy `publish`/`relist` wrappers capture only at invocation; asynchronous callers use prepared APIs.
Post-publication blob pruning retains published/current-draft hashes, validates the exact blob
namespace/key/format and rolls back on failure. Shared publication stages an owner-signed conditional
union of current and candidate photo references, then finalizes only the matching exact signed
profile target. Refresh renews live references while preserving a different pending target; media
revisions are independent of profile versions. Withdrawal uses the same staging/finalization path.
The network client retains exact verified signed-profile bytes for intent comparison: reconstructing
a decoded profile would lose unknown fields. Conditional conflicts are reclassified, never blindly
retried with an obsolete destructive list. Safe catch-up of existing replicas across missed media
revisions remains planned.
Shared desktop normalization remains separate work.
Local `publish::hide` now uses `Store::hide_profile_draft`: it re-reads the latest draft, advances
beyond all known versions with checked arithmetic, preserves stronger proof/current fields and commits
draft/search tombstone/hidden flag together. Owner mismatch, invalid state, exhaustion or any write
failure leaves that local projection unchanged. New search tombstones carry protected `search_only`
metadata and a separate signature domain; common queue publication uses `SearchRequest.withdraw`
without downgrade. Withdrawal bypasses the normal publication window but respects a server's
explicit retry deadline, including after reopening the local Store. Ordinary verification/cache
admission reject search-purpose and ambiguous legacy
unconditional tombstones, even when a peer returns them through `GetProfile`. Historical/search
verification remains explicit and never authorizes account deletion or device-directory changes.
Search merges track withdrawal version evidence separately from ordinary contact/MLS caches.
New `search_profile_head` writes use setting format 2: big-endian version (8 bytes), digest of the
complete signed record (32 bytes), then verified purpose (0 ordinary, 1 search withdrawal,
2 ambiguous legacy withdrawal). Format 1 retains the same version/digest with unknown purpose.
Opening or reading a Store never rewrites either format. Matching verified pending bytes can
resolve an unknown purpose in the read snapshot; a contradictory known purpose is invalid.
Purpose metadata is queue bookkeeping and never authorizes ordinary account/device changes.
Legacy queued Hide signatures require owner reauthorization before sending. Shared Rust now exposes
`Store::prepare_search_withdrawal_recovery` and `confirm_search_withdrawal_recovery` for hidden
legacy/unknown heads. The opaque plan retains no key, session or operation lease and signs nothing
on preparation. Confirmation binds the owner and database path, exact draft/visibility,
queue/head/ACK/backoff, observed/listed version floors and link/enrollment context. A stale review
fails without writes; an already completed same-plan retry preserves current draft edits and does
not spend another version, including after ACK. The new search-only head and updated draft version
commit atomically, preserving private fields, sealed credentials, visibility and server backoff;
the strongest available proof pair from the draft or verified pending record survives recovery.
Ordinary deletion and known new withdrawals need no recovery; a visible ambiguous head, missing
draft, pending link or exhausted version requires explicit resolution. Opening never migrates or
re-signs state. Common native exports `MobileSearchRecovery.prepare` return an optional opaque
`SearchWithdrawalReview`; `confirm` reacquires existing original-slot credentials and binding,
then delegates the local commit to core. Idle handles retain no keys, sessions or leases.
`dismiss` invalidates the shared review; generated object disposal only releases its wrapper.
`SearchRecoverySaved` reports the shared core's consistent local draft/visibility/search-pending
snapshot. Confirmation only queues bytes and captures a public signed receipt after the commit.
`Store::due_search_withdrawal_recovery` and `search::publish_recovery` admit and send only that
confirmed receipt, without signing or renewing intent. They require the original owner/database,
hidden intent, unchanged link context and exact remembered head, honor server backoff, and do no
work after ACK. A newer head requires a fresh action rather than sending it through the old review.
Native `SearchWithdrawalReview.retry` reacquires original-slot admission, checks readiness before
network access and dispatches on the dedicated network stack. Unsupported servers retain the queue;
retry returns current local state, not proof of network-wide removal. The original native sender
`send_held` is invoked only by publish/hide/withdraw; restart-safe automatic lifecycle scheduling
remains planned separately from the generic outbox entry point below. Core `Store::search_outbox_schedule` distinguishes idle,
owner-review-required and the exact Unix-second send deadline in one read snapshot.
`due_search_publication` admits only authorized signed records at that deadline; raw queue access
remains available for explicit owner recovery. Reading the schedule never signs or migrates state.
Common native `MobileSearchOutbox` retains only original-slot coordinates; `status` reopens the
existing database and returns `Idle`, `ReviewRequired` or `Scheduled { readyAt }` in Unix seconds.
`poll` reacquires the original owner, checks core readiness before network access, and dispatches
already signed bytes on the dedicated network stack. Missing databases remain idle without creation;
legacy records remain blocked and no MLS/chat credentials are created. Network errors leave the
queue intact. `MobileSearchOutboxes.poll` visits all ready owners independently without changing
selection; a failed owner does not prevent the others from running. Core `SearchOutboxWake` merges
future deadlines, returns `Retry` for failed or still-due work and excludes owner-review-required
queues from automatic wakes. Native `At` includes an exact deadline and a saturating relative delay
computed by core using the current millisecond clock. Retry takes priority over a later future
queue; platform backoff governs the next pass, which recomputes every owner's deadlines.
Android launches an initial network-constrained persisted wake from `DYApplication` and reuses
`MailJob` for deadline sends, periodic fallback and mail. Publish/hide/withdraw and network-activation
completion kick the same outbox job even after a failed or cancelled UI result. Kotlin adapts the
native wake to `JobScheduler`; failed/still-due passes request its backoff rather than an immediate
loop. Long waits are capped at `JobInfo.MAX_BACKOFF_DELAY_MILLIS` for safe OS adaptation; every
resulting pass still checks the exact core deadline before sending.
Stopping a job suppresses its late platform completion without releasing actual Rust admission.
All non-search job IDs retain mail handling, including dynamically allocated instrumentation jobs;
running callbacks are keyed by namespace and ID (namespace is absent before API 34). A final worker
callback follows the completion decision for debug-only framework instrumentation; release has no
configurable probe or backend.
OS wakes remain best effort; iOS lifecycle wiring remains planned.
Common Tauri commands `prepare_search_recovery`,
`confirm_search_recovery`, `retry_search_recovery` and `dismiss_search_recovery` use opaque string
handles and the same core policy. Idle desktop state retains only owner/path/generation and the
review; each worker holds a fresh identity lease through completion. A changed identity generation,
owner, database path or replaced/dismissed handle refuses work. Dismissing an old handle cannot
close a newer review. Confirmation and retry return the local `version`, `hidden`, `searchPending`
snapshot; gated recovery UI remains planned.
Withdrawal now prepares checked version/proof
floors before sending and projects verified readback through one immediate transaction, preserving
current draft fields/photos/devices and stronger proof. Superseded versions and equal-version
search forks refuse the local commit; remote success may already have occurred. The remembered
signed-record digest identifies the exact own tombstone after ACK and concurrent proof improvement,
so republication preserves current fields rather than falsely importing it as foreign. Photo-list release
never retries beyond the accepted tombstone version, protecting newer remote publication lists.
Publication, hidden linked-device adoption and stale media retries use checked version arithmetic
and report exhaustion rather than wrapping. Adoption checks this before fetching photos or replacing
the draft; earlier link-state import still applies. The largest visible remote version remains valid.
Adoption returns separate exact ordinary readback and current local draft/visibility. Linked-device
completion records only that ordinary published version, including its already-listed retry path;
new device listing uses a prepared relist outcome. A hidden search tombstone never proves listing.
Ordinary native and Tauri profile saves delegate directory policy to `Session::publish_profile`.
It validates the original owner/device keys, reads verified ordinary authority and copies only its
certificates/owner bindings into the captured editable request. It never treats a saved or hidden draft
as listing evidence. Verified removal refuses the original device; absent/withdrawn or unproven
authority returns `Failure::DirectoryAuthorityRequired`. A genuinely new account with no remote
directory or prior/link evidence can list its original first device. `Failure::LinkPending` requires
original-session link completion before publishing fields; a field-save click cannot complete or renew
enrollment. Device listing counters advance only from verified ordinary membership/publication.
The field-only `publish::publish_fields_prepared` path applies the same directory-source guard
without enrolling devices. Both paths preserve the original editor baseline and use ordinary field
completion, separately from directory-only completion. A changed semantic source at the subsequent
read conflicts even below a higher hidden floor; a signed wire fence then guards the write itself.
Low-level owner signing/publication and `Session::list_self` remain explicit primitives, not client
authorization checks against editable drafts. Stored listing floors never decrease to match an older
ordinary readback. `Failure::DirectoryBehindKnown` reports both the known floor and observed ordinary
version, asking for sufficiently new verified authority while retaining local data and credentials.
The same refusal applies to legacy draft-derived counters and genuine newer membership. A sufficiently
new directory resumes field publication only when it still lists this original device; missing membership
never enrolls it through field consent. Explicit owner/relink recovery when such authority is unavailable
remains planned.
Unlisted-link completion prepares a directory-only request from exact adopted public fields and adds
the restored device, retaining the local version floor and stronger proof. It checks revocation at
the exact ordinary readback before applying that floor. The next read must still match the captured
public source or the exact intended target; a different source conflicts without replacing local
pending values, even below a higher local/search version. Completion retains current editable draft
fields and applies directory results against the actual local baseline, including later directory
edits. Draft/search/privacy/retention and receipt remain one atomic completion. Private pending photo
bytes are retained locally and never uploaded by this directory-only request.
Prepared publication, relisting and withdrawal use `conditional_publish` with protected
`Profile.previous` metadata signed in the `ProfileConditional` domain. The fence binds the last
ordinary version and SHA-256 of its complete encoded signed record, retaining original payload bytes.
Each replica atomically refuses replacing newer state or a different record at the observed version;
missing/older replicas may seed the target. This is a per-replica fence, not global consensus.
A wire conflict returns `Failure::Missed` and retains local pending data. Exact accepted readback
after a lost reply reuses its original fence; local completion/search re-signing retains the domain.
Older nodes return `UNSUPPORTED`; clients never retry conditional authorization as legacy publish.
The explicit low-level legacy publication primitive remains available for existing authorizations.
Device revocation captures its actual local baseline and visibility before the first network await.
Its directory-only request always starts from verified public readback, even when the local or caller
version is equal or higher. The caller draft supplies only a version floor or an authenticated owner
binding retained for retry; editable caller/local fields are never implicitly published. Completion
retains both pre-existing pending fields and later local edits, stronger proof and protected photos.
If the remote directory already omits the target, a retained binding permits repeated mailbox close
and atomic directory-only import without another public version. Exact receipt replay retains the
current search version, ACK state and receipt while applying the directory change. Missing/removed
public profiles and unknown bindings fail without republishing local data. A foreign source observed
after closure conflicts without replacing pending fields; closure or remote publication can precede
a later failure. Ordinary and hidden completion version exhaustion is checked before closure.
Directory readback is checked against persisted ordinary observation/listing versions before closure
and at final readback. Imports repeat that check under the writer, including exact receipt replay;
a higher editable/search version never makes an older ordinary profile current.

Ordinary foreign publication conflicts now project through a prepared Store import:
unchanged baseline fields yield to the winning remote profile, while post-prepare edits remain pending.
The transaction preserves current visibility, paired device-directory edits, stronger proof and protected
photo bytes; its search record contains exact remote content or a higher hidden tombstone, never pending
local edits. Newer known versions, missing prior drafts, wrong owners and equal-version search forks
refuse the local projection. Draft/search/photo retention roll back together on failure. The caller still
reports `Failure::Missed`, so clients must reload the local draft rather than claim it was published.
Ordinary linked-device adoption captures a baseline after the intentional link-state import and before
its first network await. Its completion uses the same import projection and atomically records the
exact remote observed-version clock alongside draft/search/photo retention. Visibility and version
floors are re-read under the writer, including hidden-version exhaustion after photo fetching; failures
roll back completion writes, while the earlier link-state prefix remains applied. After exact replay
and narrowly proven legacy replay, a different readback cannot replace differing pre-existing editable
fields: adoption returns the typed owner-review outcome with draft, search, receipt and photo retention
unchanged. The completion digest alone is not an editable merge base. Protected version/proof/directory
differences do not by themselves require field review. Confirmation still requires the original device
certificate in the verified remote directory; owner field choices never restore an unlisted or revoked
device. Initial adoption without a saved draft and post-prepare edits retain their existing behavior.
`Session::resume_link` consumes an existing session, verifies owner/device and the supplied sealing key
against its saved device before importing pending state, and reuses the ordinary directory/key-package/
fallback completion. It never creates credentials or a session. The pending row goes last; on failure,
reopen the original session and retry. Common native `ProfileAdoptionReview.resume_link` delegates to
this operation; client presentation remains planned.
New/replaced pending links atomically retain fresh enrollment provenance. Before an unlisted device's
first relist request, shared core commits an intent bound to the exact pending snapshot, original
certificate and verified semantic ordinary source, checking the operation's original pending bytes
under the writer so replacement cannot supply a new grant to an older attempt. It never records
speculative `device_listed` membership. An unchanged source allows a retry; a current listing proves acceptance even if the local
proof write previously failed. A changed source without the certificate, or a legacy pending link
without enrollment provenance, requires new owner authorization (`LinkEnrollmentReviewRequired`).
Repeatedly saving the same pending bytes cannot renew that authorization. This conservatively refuses
a changed source even if the original request never reached the server. Actual listing proof also
refuses contradictory absence at the same ordinary version; hidden draft versions remain unrelated.
This journal is local admission evidence, not a wire digest or server-side compare-and-swap token.
Adoption checks the Store owner before any private-state prefix import. Export reads peer, like/pass,
HLC and visibility values from one read snapshot without changing their wire ordering or actor fields.
Pending link creation atomically records exact private-state bytes and a format-1 import phase bound
to their SHA-256 digest. Setting the same pending bytes keeps that phase; a new/replaced operation
resets it. Private peer/like/pass state, HLC, visibility and the applied phase commit together. Exact
applied retries leave later local/synchronized changes untouched, including after process reopen.
Final pending completion clears only its own exact applied prefix and import phase under the writer,
retaining the linked-device marker. A replaced or unapplied pending operation returns `LinkStateChanged`
without clearing the new link.
An old pending row without a phase can record applied progress without replay only when its raw bytes
exactly match the current canonical ordered private snapshot under the writer. This comparison includes
visibility, trust observations and actor/HLC metadata; unknown wire fields also prevent admission.
Any difference returns `LinkStateReviewRequired`, preserving both snapshots and credentials.
`Store.prepare_link_state_review` now freezes the owner, database path, exact pending bytes and current
private snapshot in one read transaction. Its read-only differences cover block/mute/favorite/name/note,
pass, visibility and the owner's outgoing like; incoming likes, trust/device observations and historical
actor/clock/event metadata are never offered as editable preferences. A separate protected-difference
signal also detects unknown/noncanonical pending wire data. Duplicate peer/like/pass keys are refused.
`LinkStateReview.is_current` checks the same pending operation, absent import progress and current
private bytes; it holds no keys or identity lease and authorizes no writes. Confirmation must revalidate
under its writer with original-device admission. `select_preferences` requires one explicit Local/Pending
source for every differing owner preference, rejects duplicate/unoffered choices and returns a frozen
preview in core presentation order. It exposes selected values without historical actors/clocks/event IDs;
a preview neither imports state nor resolves protected evidence or grants enrollment. Atomic confirmation,
protected-evidence merging and native/desktop adapters remain planned.
A standalone fresh-store import can replay its exact snapshot; a different source needs a new pending
operation. A profile completion receipt or version offset never proves private-state import.
Own publication and import atomically store a format-1 `profile_completion_receipt`: one visibility
byte, the SHA-256 digest of the canonical owner-signed ordinary profile, then its signed search
projection. This local receipt compares decoded profile content; media intent checks retain original
verified wire bytes separately.
A repeated import preserves the complete pending draft only when that readback, visibility and current
search head still match, including after ACK. This admits the confirmed hidden successor without
accepting an arbitrary older/forked profile or spending another version. Without a receipt, a legacy
hidden-link completion can migrate only if its listed-device version matches ordinary readback,
all decoded draft fields match that readback (apart from the hidden successor and stronger local proof),
and the exact signed search head matches the local or remote proof. The receipt is installed under the
same writer; pending content changes or ambiguous evidence refuse automatic migration. Adoption
reports typed `publish::Failure::AdoptionReviewRequired` for a superseded local projection, preserving
the pending link, draft and credentials. Callers must distinguish this from a transient transport
failure; blind retries do not resolve it. The planned recovery compares local and verified remote
profile fields and requires explicit owner selection; device authorization stays remote-authoritative.
Shared `publish::review_adoption` now freezes an owner-bound local draft/visibility before fetching
verified ordinary readback. `AdoptionReview` owns no identity lease or database transaction while the
owner reviews it. Its shared field vocabulary keeps birth, place, income and seeking groups indivisible;
`select_fields` requires one explicit local/remote choice for every differing editable field and rejects
missing, duplicate or unoffered choices. Selection produces a preview only: remote devices, owner
bindings and metadata remain authoritative, stronger local proof survives, and storage is unchanged.
The vocabulary exhaustively classifies wire fields so new fields require an explicit policy decision.
The review captures draft, visibility, pending-link bytes, exact signed search head, observed/listed
versions and database identity in one read snapshot. `is_current` rejects a replaced link, another
Store (even for the same owner), or changed profile/publication progress; ACK/backoff for the same
head does not invalidate the review. Reopening the same Store retains validity. This read-only
preflight holds no transaction after return and is not a write authorization.
`confirm_adoption_review` validates explicit choices and the actual restored session certificate,
requires that exact certificate in reviewed remote authorization, fetches missing selected photo
bytes, rechecks ordinary readback, then repeats local matching under the completion writer.
It atomically saves the selected local draft, privacy-preserving search projection, retention and
completion receipt. Selected local values remain pending; public profile publication is the normal
separate consent-gated action. A hidden draft advances above local/search floors with a tombstone;
a visible draft queues only exact already-published remote content. Pending-link state stays until
existing key-package/fallback completion succeeds. Changed context requires new review and cannot
erase newer local edits; an unlisted credential is not automatically restored.
The common native `MobileLinkRecovery(registry, id, expected_peer, store)` binds existing credentials
without opening them. Its blocking `prepare` returns an opaque `ProfileAdoptionReview`; no key,
session or operation lease stays in either idle handle. `snapshot` includes complete editable local
and remote values, all original photo chunks, visibility and typed differing field groups.
`confirm(choices)` reacquires existing-only admission, checks the original session certificate and
local context and core `ChatBinding` (slot/owner/device plus sealing-key digest), rejects an already
changed active network, and delegates all field selection and atomic completion to core. Typed errors
separate profile-review-required, private-state-review-required, enrollment-authorization-required,
invalid selection/profile, unlisted,
revoked-device and backend failures. Confirmation atomically records the exact ordinary version
proven to list the original session certificate alongside draft/search/retention/receipt changes,
including from an initially zero listing counter. Later readback cannot mistake its removal for
first enrollment. The worker continues using the originally reviewed network;
UI cancellation does not drop its admission before actual completion. The credential binding is
not a filesystem generation counter and does not identify an identical externally restored copy.
`resume_link` reopens the original existing session and checks its slot/key binding, certificate,
network and exact original pending bytes. At actual resumption, core
`AdoptionReview::remember_device_membership` retains the comparison's verified historical membership
under the original owner/database/pending writer, including when no field confirmation was needed.
Preparation itself stays read-only. This observation grants no field consent or device restoration;
a subsequent refusal may require a fresh field comparison because the factual listing counter advanced.
It delegates directory listing, key-package/fallback
publication and final pending completion to core. Post-confirmation editable profile fields remain pending; resumption
cannot bypass a required owner choice or publish editable values implicitly. A missing pending row
returns false without connecting. Failed fallback publication keeps the exact saved signed record
for retry, and a replacement pending link is never cleared by the older completion. Admission stays
alive until the actual session/future finishes, including after UI handle disposal.
Tauri adapters, presentation, renewed-owner authorization for uncertain legacy enrollment, and explicit
legacy private-state recovery remain planned. Automatic
pre-receipt recovery never treats a malformed or nonmatching existing receipt as absent. Remaining
client prepared-outcome adapters still require migration before frontend monitors can be removed.
Search queue hold/ACK/backoff operations also use immediate writer transactions. A durable
version/signed-record digest prevents stale requeue after ACK/restart and rejects equal-version
forks; timestamps/backoff are monotonic. Local hide composes the queue helper inside its own
transaction, including the remembered version floor. Own publication/relist completion uses that
same queue helper inside the draft/visibility/blob transaction.
Its exported API is unchanged.

## Profile editor height and seeking input

`parse_profile_height` and `parse_profile_seeking` delegate to
`dyapp_profile::editor_input`. Blank height clears the field; invalid or out-of-range input
is rejected before saving with the existing field error. Seeking ages use core limits and
known gender choices; gender ordering and not-looking normalization happen in core.
Not-looking ignores and clears hidden input even if malformed. Kotlin renders the returned
normalized value after a successful save. Draft save reuses the same core seeking validator,
so bypassing the editor cannot bypass validation. The FFI layer only converts inputs/results.

## Raw birth field input

`parse_profile_birth_parts(year, month, day)` and `valid_profile_birth_parts(values)` delegate
to `dyapp_profile::birth_input`, which reuses `Profile::valid_birth_parts`. Empty parts remain
absent; invalid integers, unknown keys and impossible partial dates are rejected. Kotlin
retains field keys and rendering only. Tests crossing this adapter belong in Android
instrumentation, where the native library is available, rather than host JVM-only tests.

## Raw search range input

`search_range_conditions(offered, SearchRangeInputs)` interprets the Android form's raw age
and height strings through `dyapp_profile::search_input`. The core owns parsing, hidden-field
exclusion and defaults; FFI only converts records and calls the existing query validator.
Desktop can use the same core module directly without a UniFFI dependency. Blank ages add no filter unless either bound is entered; a missing
bound then defaults to 18 or 150. Blank height bounds remain open. Hidden fields are ignored,
including malformed saved input. Offered malformed or impossible ranges retain the existing
`invalid search age` / `invalid search height` errors. Date-dependent bounds use the current
UTC date and the existing core `Filter` validator. Categorical, place and mutual form
interpretation remains client-owned pending the rest of the extraction.

`dyapp_client::search_input::employer_filter` interprets raw Any/Same/Exclude company text using
the Linux desktop contract. Same preserves semicolons; Exclude splits on them, drops blank names
and preserves order/duplicates. Matching remains local through `Employer::retain`, never node
conditions. Raw search ranges, composer and employer inputs share
`dyapp_profile::text_input::trim`, the ECMAScript predicate: BOM is blank, NEL is retained.
The same predicate controls numeric parsing and blank detection; hidden invalid ranges remain
ignored. The common native `search_employer_filter(SearchEmployerMode, text)` only converts
enums and delegates to the core. Android and iOS pass raw field text through it, retaining mode
labels and rendering only. Employer tests crossing the native boundary belong in Android
instrumentation. Desktop adoption remains planned under the shared-query extraction.

`dyapp_client::search_input::SearchChoiceInputs` builds offered categorical, country,
interest and selected-place equality conditions. It uses canonical scale order, ignores unknown
keys and zero choices, and trims raw text with the same Linux predicate. Interest tags then
pass through `dyapp_profile::normalize_interest` (trim, lowercase, NFC), matching publication;
its domain normalization also removes NEL at the edges. Country and employer retain their
separate raw-input whitespace contracts. Selected place IDs are
independent of country-field availability; pickers must invalidate a selection when its source
country/text changes. Free-form place labels are never wire conditions. The common native
`search_choice_conditions` adapter converts records only; Android and iOS use it for these
choices and append range conditions separately. Choice conditions use canonical scale order,
then country, interest and place. Android restores selected UInt32 IDs from the picker's Int
storage with `toUInt`, preserving all bits. Desktop adoption remains planned. Interest input
is supported by the shared API; the native forms currently pass blank interest text.

## Identity registry core

`dyapp_client::identity_slots` reads the existing desktop, Android and iOS inventories without
opening keys, writing markers or migrating profiles. The layout selects desktop storage-mode
markers or mobile wrapped-key files; iOS sibling deletion markers remain visible after a slot
directory disappears. Active pointer syntax preserves absent, invalid, explicitly unselected and
selected states; Android's existing pointer without LF remains readable. Candidate order is
legacy first, then canonical lowercase UUIDs. Symlinked slots/markers cannot become ready, and
a symlinked inventory root fails.

The shared `resolve`/`open_slot` layer opens existing keys without generating replacements or
writing markers. Core validates regular key files, desktop private-file permissions and bounded
lengths; `KeyReader` only reads system credentials or decodes supplied wrapped bytes. Pending
and deleting slots remain recovery even when their Peer ID is known. Empty inventories with
orphan legacy credentials, missing selected targets or invalid saved pointers cannot become
fresh setup. iOS explicit unselection remains a picker, and a missing selected legacy key never
silently switches to a UUID identity. Callers must serialize resolution with mutations.
`Registry` serializes inventory and resolution through the existing desktop `profiles.lock`
or iOS `.identity-slots.lock`; Android uses `profiles.lock` when adopted. The guard remains held
through backend callbacks and releases on failure. Construction does not write; operations may
create the root and lock file, never an identity. Non-regular lock paths fail closed.
`Registry::create` reserves a fresh UUID under that lock, writes `pending` before asking the
platform to store credentials, saves key files exclusively, verifies the reopened Peer ID, then
writes `ready` and finally the active pointer. `KeyWriter` returns wrapped bytes or stores a
fresh system credential; it must reject existing aliases. A failed verification leaves pending
metadata and the old selection. An active-pointer write failure can leave a verified ready slot
without changing the old pointer when failure precedes the rename. A directory-sync failure
after rename is returned as `SelectionDurability::Unconfirmed` alongside the committed slot
and identity; it is not a refused selection. `Registry::create` and `Registry::select` return
that status explicitly. Errors from the active-pointer step occur before rename and preserve
the prior pointer. Unix creation syncs parent directory entries as well as
file contents before advancing; Windows currently has no directory-flush guarantee in this
portable API. Stranded `.tmp` files fail closed rather than being followed,
truncated or deleted. `Registry::select` finds the requested ID in the current inventory,
requires ready metadata and opens its key before atomically changing the pointer; invalid IDs,
incomplete slots and unavailable keys preserve the prior selection.
`Registry::link` turns an owner's link record into a new slot under the same lock. It
reserves a pending slot in the storage mode of the slot that waited, saves and reopens the
linked identity, then calls the client's device-key step, since desktop and mobile keep that
key differently. It copies the waiting slot's network settings, stores the link state in the
new database, writes `ready` and only then the active pointer. Any failure before that leaves
the new slot pending and the old selection in place. A waiting slot made only to link is
removed afterwards when it has no saved profile draft; a failed cleanup does not fail the
link and stays visible as a deleting slot. The desktop link flow calls it; the FFI wrapper
and mobile adoption remain planned.

## Exported surface

Authored Rust exports now carry API documentation, including explicit stub behavior.
`missing_docs` is denied in `ffi` too ([B9](../development/code-quality.md#public-api-documentation));
the uniffi scaffolding passes it unchanged.

All exports, with what they really do. "Stub" means the body ignores its inputs
or returns constants.

| Rust export | Kind | Signature | Behaviour |
|-------------|------|-----------|-----------|
| `PeerConnection` | Object | `new(peer_id: String)`, `get_id() -> String` | Holds the string; no networking. |
| `MessageService` | Object | `new()` | Stub. |
| | method | `send_message(peer_id: String, text: String) -> Result<String, CoreError>` | **Stub.** Drops `text`, returns `"Message queued to <peer_id>"`. Nothing is queued, encrypted or sent. |
| | method | `queue_size() -> i32` | Stub, always `0`. |
| `VideoSession` | Object | `new() -> Result<Self, CoreError>` | Creates a dedicated Tokio runtime. No peer ID. The WebRTC session itself is created lazily by `create_offer` or `set_remote_offer`. |
| | method | `create_offer() -> Result<String, CoreError>` | Creates the inner session on first call, returns an SDP offer. |
| | method | `mark_offer_sent() -> Result<(), CoreError>` | OfferCreated → OfferSent. |
| | method | `receive_answer(sdp: String) -> Result<(), CoreError>` | Parses and applies the remote answer, then applies stored remote ICE candidates (see [video](video.md)). |
| | method | `set_remote_offer(sdp: String) -> Result<(), CoreError>` | Callee side: applies the caller's offer, then stored remote ICE candidates; Idle → OfferReceived. |
| | method | `create_answer() -> Result<String, CoreError>` | The SDP answer to that offer; OfferReceived → AnswerCreated. |
| | method | `end_session() -> Result<(), CoreError>` | Closes the session; not `close`, which clashes with `AutoCloseable.close()` in the Kotlin bindings. |
| | method | `get_session_id() -> Result<String, CoreError>` | Session UUID. |
| `KeyStore` | foreign trait | `wrap(secret: Vec<u8>)`, `unwrap(wrapped: Vec<u8>)`, both `-> Result<Vec<u8>, WrapError>` | Implemented by the app over its platform keystore; encrypts with a key that never leaves it. |
| `IdentityKey` | Object | `generate()`, `unwrap(store: KeyStore, wrapped: Vec<u8>) -> Result<Self, CoreError>` | A `rust/identity` key; `unwrap` fails unless the store returns 32 bytes. |
| | method | `wrap(store: KeyStore) -> Result<Vec<u8>, CoreError>` | The secret encrypted by `store`, for the app to persist. |
| | method | `peer_id() -> String`, `public_key() -> Vec<u8>` | Hex SHA-256 of the public key; the 32 public key bytes. |
| `MlsDevice` | Object | `new(identity: IdentityKey, device_key: IdentityKey, generation: u64)`, `open(store: KeyStore, wrapped_key: Vec<u8>, sealed: Vec<u8>)`, both `-> Result<Self, CoreError>` | A `rust/messaging` MLS device with a random state key; `open` restores one from its sealed state. |
| | method | `wrap_state_key(store: KeyStore)`, `seal()`, both `-> Result<Vec<u8>, CoreError>` | The state key encrypted by `store`, stored once; all MLS state sealed under it, stored after every call that changes state and before sending its output. |
| | method | `certificate() -> Vec<u8>`, `key_package(last_resort: bool) -> Result<Vec<u8>, CoreError>` | The encoded signed certificate for the profile's device list; a KeyPackage to publish. |
| | method | `create_group(key_packages, profiles) -> Result<MlsNewGroup, CoreError>`, `join(welcome, profiles)`, `load_group(id) -> Result<Option<MlsGroup>, CoreError>` | `profiles` are encoded signed profiles, the newest the app holds for every member's owner, verified in Rust. |
| `new_chat_state_key` | function | `(store: KeyStore) -> Result<Vec<u8>, CoreError>` | A new MLS state key encrypted by `store`; the app persists it once per identity. |
| `ChatSession` | Object | `open(dir: String, key: IdentityKey, device_key: IdentityKey, store: KeyStore, wrapped_state_key: Vec<u8>) -> Result<Self, CoreError>` | One identity folder's chats in its `client.db`. The first open certifies a device with the mailbox key `device_key` and seals it there; later opens restore it. A different state key or device key is refused. Calls block and run off the UI thread. |
| | method | `publish_profile(attested: bool) -> Result<ProfilePublished, CoreError>` | The publish below with this device listed in the profile, then a batch of KeyPackages so peers can invite it; a node already holding the device's full stock (20) ends the batch as done. `full` when the node's media quota has no room for the photos. |
| | method | `state(peer_id: String) -> Result<ChatState, CoreError>` | `None`, `RequestSent` (`chat-first`), `RequestReceived` (`chat-request`), `Open` or `Closed` (`chat-closed`), as in [chats and matches](../design/chats-and-matches.md). |
| | method | `send_text(peer_id: String, text: String) -> Result<Vec<u8>, CoreError>` | The text's 16-byte logical ID. Fetches the peer's verified profile from the active network and goes to the pair's lowest-ID group; the first text invites the peer, a text to a request is the reply that opens the chat. A failed send stays staged. Errs `ownPeer`, `invalidText`, `blocked`, `unpublished` (the own profile does not list this device, or the peer has no published profile), `notConnected` or a node or storage message. |
| | method | `photo_download() -> Result<ChatPhotoDownload, CoreError>`, `set_photo_download(mode: ChatPhotoDownload) -> Result<(), CoreError>` | Persisted per-identity original-download mode: ByNetwork (default), Always or OnTap. Shared by chat and profile photos; stranger protection still overrides Always. |
| | method | `send_chat_photo(peer_id: String, photo: Vec<u8>, preview: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, CoreError>` | Prepared bounded JPEG and optional validated JPEG preview, sent through core sealing/upload to an open conversation only. Returns logical message ID. No staging on upload refusal; delivery failure after upload may leave a stored photo, so compare attachment history before retrying. Errors include `ownPeer`, `blocked`, `conversationNotOpen`, `unsupported`, `corrupted`, `full`, `tooLarge`, `peerUnavailable` and network/storage failures. |
| `chat_photo_hours_left` | function | `(sent_ms: u64, now_ms: u64) -> Option<u32>` | Core retention estimate in whole hours, rounded up, only during the last day. Zero means expired; it does not prove a kept original is unavailable. |
| | method | `chat_items(peer_id: String) -> Result<Vec<ChatItem>, CoreError>` | Core-ordered text and photo history, without network work. `ChatItem` is Text (existing `ChatMessage` with read marks) or Photo (`ChatPhoto`). Preserve this order rather than sorting by wall time: the core includes HLC logical ticks and identity tie breakers. Blocked peers err `blocked`. |
| | method | `chat_photos(peer_id: String) -> Result<Vec<ChatPhoto>, CoreError>` | Local attachment metadata in core attachment order; blocked peers err `blocked`. `ChatPhoto` retains group/author/message identity, full HLC, wall time, size, MIME, bounded preview, clamped layout dimensions and unread/forward/delivery flags. No encryption or release secrets cross FFI. Preview bytes still require bounded native decoding. Original bytes are requested by stored message identity below. |
| | method | `chat_photo_original(peer_id: String, group_id: Vec<u8>, author: String, id: Vec<u8>) -> Result<Vec<u8>, CoreError>` | Looks up the stored descriptor by all message identity fields. Reads sealed local originals first, otherwise fetches and authenticates/decrypts through the core, validates bounded image bytes and best-effort caches them. No recipient release. Errors distinguish `blocked`, `notFound`, `corrupted`, `notConnected` and retryable transport failures. Call after a tap or an allowed auto-download decision, off the UI thread. |
| | method | `change_sticker_pack_request(peer_id: String, group_id: Vec<u8>, author: String, id: Vec<u8>, action: StickerPackSaveAction) -> Result<StickerPackSaveAttempt, CoreError>` | Apply a core received-pack action offline; preserve completion if later status is unavailable. |
| | method | `chat_sticker_pack_status(peer_id: String, group_id: Vec<u8>, author: String, id: Vec<u8>) -> Result<StickerPackSaveStatus, CoreError>` | Full-key received-pack status from shared core. Typed state/failure, current intent generation and exact retry deadline; no descriptor, key or raw peer text. Can reconcile an already-saved intent under the native session lease; call off the UI thread. Own-authored/invalid/blocked sources refuse, unreadable content stays explicit Failed/Storage, and a failed lookup is never New. |
| | method | `chat_sticker_frame(peer_id: String, group_id: Vec<u8>, author: String, id: Vec<u8>, manual: bool) -> Result<ChatStickerFrame, CoreError>` | Full-key current-message retrieval through shared media snapshots. Authenticated local originals/saved packs precede network access; automatic download obeys core policy and the current OS report, while manual means an explicit tap. Returns actual dimensions and bounded RGBA8 only, with no compressed peer media or transport descriptor. Core revalidates current identity/content before completion/cache retention. Keep the UI generation guard after cancellation. Errors distinguish onTap, notFound, current-message refusals and network failure. |
| | method | `chat_photo_auto(peer_id: String) -> Result<bool, CoreError>` | Whether full chat originals may load without a tap now: shared download setting, current platform network report and an open conversation are required. Strangers never auto-download, even with Always; blocked peers err `blocked`. Original retrieval uses the separate method below. |
| | method | `receive() -> Result<u32, CoreError>` | Over the active network: resends due staged writes, applies this device's mailbox, runs bounded core pack responses/admitted downloads (completed saves add to refresh count) and refills tokens when due, then, best effort as on the desktop, keeps the last-resort key package and runs the daily photo keep refresh. Returns how many events changed a chat. Senders' profiles are fetched from the node on each call; an envelope whose owner has none there waits in the mailbox for the next call. |
| | method | `chat_rows() -> Result<Vec<ChatRow>, CoreError>`, `chat_history(peer_id: String) -> Result<ChatHistory, CoreError>` | The local projection, no network. Rows leave out blocked peers; a history for a blocked peer errs `blocked`. Both carry `muted`. In a history, `read` on an own live text says the peer's read receipt covers it (the desktop rule); rows leave it false. |
| `sticker_import_byte_limit` | function | `() -> u64` | Authoritative core picked-TAR byte bound; use it when reading an OS-selected stream. |
| | method | `import_sticker_pack(archive: Vec<u8>) -> Result<SavedStickerPackImport, CoreError>` | Core-validated local TAR import into this identity's sealed collection. Success returns validated PackID and added=true for a new saved pack, false for an already readable identical pack. Persistence errors can follow file publication: refresh authoritative collection state rather than infer rollback. No network or chat staging. Call off the UI thread. |
| | method | `saved_sticker_packs(after: Option<Vec<u8>>) -> Result<SavedStickerPackPage, CoreError>` | Identity-local bounded collection page from core storage. Entries retain PackID, encrypted size and Ready { name, stickers } or Unreadable { detail }; page retains the exclusive cursor, all-file used bytes and unrecognized-file count. A supplied cursor must be exactly 32 bytes. No networking, deletion, compressed media or transport secrets. Call off the UI thread. |
| | method | `saved_sticker_frame(pack_id: Vec<u8>, index: u32) -> Result<SavedStickerFrame, CoreError>` | One authenticated saved entry, decoded by core to actual width/height and row-major RGBA8. Retains PackID, manifest index, emoji and compressed-content hash without compressed bytes, keys or release capability. No networking or file mutation; invalid/missing selections and unreadable files refuse. Call off the UI thread; scale the returned pixels for presentation. |
| | method | `submit_saved_sticker(peer_id: String, pack_id: Vec<u8>, index: u32) -> Result<ChatSubmission, CoreError>` | Send an authenticated saved-pack entry through core upload and open-chat submission. A queued result retains its committed ID; never resubmit it. Pre-commit failures remain errors, even if an upload reservation was persisted. Invalid hash length, missing pack and invalid index fail before network access; foreign or corrupt saved files are preserved. Call off the UI thread. |
| | method | `forward_text(source: ChatForwardSource, target_peer_id: String) -> Result<ChatSubmission, ChatForwardError>` | Forward current local text into another open chat, as on Linux. Source carries peer, group, stored author and logical message ID; core verifies the full live key, source visibility and target eligibility. UI never supplies the forwarded body or a verified original-author claim. The new message is authored by this identity and carries the existing forwarded marker. Typed errors are pre-commit refusals/backend failure. `Queued` is already committed with its stable ID and exact retry bytes; do not submit a second copy. |
| | method | `contact_note(peer_id: String) -> Result<String, ContactNoteError>`, `set_contact_note(peer_id: String, note: String) -> Result<(), ContactNoteError>` | Identity-bound private notes without a profile fetch or peer signal. Core preserves clean NFC text, case and LF lines up to 2000 code points, and stages own-device sync. Empty text explicitly clears; blocked contacts retain read/write access. Typed errors are `InvalidPeer`, `OwnPeer`, `InvalidText` or `Failed { reason }`; a failed read is never empty. Notes and favorites require canonical lowercase peer IDs at the shared core application boundary; signed wire validation and existing stored rows are unchanged. |
| | method | `favorite_peers() -> Result<Vec<String>, FavoriteError>`, `favorite_state(peer_id: String) -> Result<bool, FavoriteError>`, `set_favorite(peer_id: String, saving: bool) -> Result<(), FavoriteError>` | Private identity-bound favorites. The shared client core validates peer IDs, rejects self and blocked saves, and stages own-device sync without contacting the peer. Listing hides blocked peers; state reads and removal remain offline and work for them. Typed errors are `InvalidPeer`, `OwnPeer`, `Blocked` or `Failed { reason }`; storage failure is never an empty list or false flag. Tauri uses the same core policy. |
| | method | `likes(peer_id: String) -> Result<ChatLikes, CoreError>`, `liked_you() -> Result<Vec<String>, CoreError>` | Local. `ChatLikes` is `ours`, `matched` and `passed`. `liked_you` lists the peers who like this profile, not liked back, not blocked and not passed, as Peer IDs: the screen fetches their profiles. |
| | method | `like(peer_id: String, liked: bool) -> Result<ChatLikes, CoreError>` | Fetches the peer's verified profile from the active network and sends a like or an unlike in the pair group; the first like invites the peer, an unlike with no group sends nothing, a match opens the chat. A failed send stays staged. Errs `unpublished` (the own profile does not list this device), `notConnected` or a node or storage message. |
| | method | `clear_chat(peer_id: String)`, `mark_read(peer_id: String)`, both `-> Result<u32, CoreError>` | Local. Clear chat removes every message with the peer on this device and returns how many went; mark read marks the peer's texts read and returns how many were unread; `send_reads` sends the receipts. Both err `ownPeer`, `blocked` or `unknownPeer` (no chat state). |
| | method | `edit_text(peer_id, group_id: Vec<u8>, message_id: Vec<u8>, text: String)`, `delete_text(peer_id, group_id, message_id)`, both `-> Result<Vec<u8>, CoreError>` | Over the active network: edits or deletes a live text this identity wrote in that group, and returns the edit's or delete's 16-byte ID. A failed send stays staged. Err `invalidText` (edit only), `ownPeer`, `blocked`, `unknownMessage`, `notConnected` or a node or storage message. |
| | method | `block(peer_id: String) -> Result<ChatBlock, CoreError>`, `unblock(peer_id: String) -> Result<(), CoreError>`, `blocked_peers() -> Result<Vec<String>, CoreError>` | Blocks or unblocks here and stages the change for the owner's other devices. A block then revokes the write tokens the peer holds; `ChatBlock` is `revocation` (`NotNeeded`, `Pending` or `Confirmed`) and `detail`, why it is pending. The block stays whatever the revocation does, and blocking again repeats it. `blocked_peers` lists the blocked peers in Peer ID order. Err `invalidPeer` (not a Peer ID, or the own one) or a storage message. |
| | method | `set_muted(peer_id: String, muted: bool) -> Result<(), CoreError>` | Mutes the peer or ends that, here and staged for the owner's other devices. Only notifications change, never the mail, the chat or its unread count. Errs as `unblock`. |
| | method | `set_contact_name(peer_id: String, name: String) -> Result<(), CoreError>` | Gives a peer whose chat shows a local name only this identity sees, trimmed, staged for the owner's other devices; an empty name removes it. Errs `ownPeer`, `blocked`, `unknownPeer`, `invalidName` (over 64 characters or control text) or a network message. |
| | method | `set_passed(peer_id: String, passed: bool) -> Result<ChatLikes, CoreError>`, `passed_peers() -> Result<Vec<String>, CoreError>` | Passes on the peer or undoes that, here and staged for the owner's other devices, and returns the like state after it; no signal goes to the peer. A passed peer leaves search results and Liked you. `passed_peers` lists them in Peer ID order. Errs as `unblock`. |
| | method | `notice() -> Result<ChatNotice, CoreError>`, `notice_content() -> Result<ChatNotice, CoreError>`, `set_notice(notice: ChatNotice) -> Result<(), CoreError>` | Local. The new-mail notice setting in the same `client.db` keys as the desktop privacy screen: `Off`, `Fixed` (the default, "New message" only), `Name` or `Text` (name and message text). `Off` keeps the chosen content for a later turn-on, and `notice_content()` reads it even while off (never `Off`). |
| | method | `read_receipts() -> Result<bool, CoreError>`, `set_read_receipts(enabled: bool) -> Result<(), CoreError>`, `send_reads(peer_id: String) -> Result<u32, CoreError>` | The opt-in read receipt setting, off by default, in the same `client.db` key as the desktop. `send_reads` sends what the shown chat owes in an accepted conversation (the latest partner text of each group) and returns how many went out; with nothing due, always the case while off, it opens no node connection. Errs as `mark_read`, then `notConnected` or a node or storage message. |
| `ChatRow` | Record | `peer_id`, `state: ChatState`, `last_message: Option<ChatMessage>`, `unread_count: u64` | One Chats row; no last message after Clear chat. |
| `ChatHistory` | Record | `state: ChatState`, `messages: Vec<ChatMessage>` | One peer's current conversation, possibly empty. |
| `ChatMessage` | Record | `group_id`, `author`, `id: Vec<u8>`, `text: Option<String>`, `edited`, `deleted`, `unread`, `forwarded`, `delivery: Option<ChatDelivery>`, `sent_at_ms: Option<u64>` | Current text or deletion marker in the Store's pair-wide order. `delivery` is `Pending`, `NodeAccepted` or `DeviceReceived` on an own message; node acceptance alone is not recipient delivery. |
| `MlsNewGroup` | Record | `group: MlsGroup`, `welcome: Vec<u8>` | The new group and the Welcome for the added devices. |
| `MlsGroup` | Object | `id()`, `members()`, `encrypt(device, plaintext)`, `process(device, message, profiles) -> Result<MlsReceived, CoreError>`, `remove_revoked(device, profiles)`, `export_secret(device, label, context, length)` | One group; each call takes its `MlsDevice`. `MlsReceived` is `Message { sender, plaintext }` or `Commit`. |
| `hash_app_pin` | function | `(pin: String) -> Result<Vec<u8>, CoreError>` | Six-to-64 ASCII decimal digits to a 49-byte record: format 1, random 16-byte salt, 32-byte Argon2id tag (64 MiB, three passes, four lanes). Store the record in device-only protected storage. |
| `verify_app_pin` | function | `(pin: String, record: Vec<u8>) -> Result<bool, CoreError>` | Constant-time tag comparison; `false` for a wrong PIN, error for invalid PIN/record or KDF failure. The platform counts failed attempts, including errors. Both calls block and belong off the UI thread. |
| `chat_link_hosts` | function | `(text: String) -> Vec<ChatLinkHost>` | Shared display-only HTTP(S) destination extraction, without credentials. Records contain `ascii: String` (including a nondefault port) and `unicode: Option<String>` as an additional IDN spelling; keep ASCII visible. No network access or automatic opening. Mobile consumption remains planned. |
| `profile_link_text` | function | `(peer_id: String) -> Option<String>` | The chat message text that shares a profile, `dyapp-profile:1:<Peer ID>`; none unless the Peer ID is one lowercase 64-hex value. |
| `profile_link_peer` | function | `(text: String) -> Option<String>` | The Peer ID when a message body is exactly one profile link; other text gives none. Open it through the verified profile fetch. |
| `make_preview` | function | `(rgba: Vec<u8>, width: u32, height: u32, max_bytes: u32) -> Result<Vec<u8>, CoreError>` | JPEG preview of pixels the app decoded: long side ≤ 320 px, quality 80 down to 40 until it fits `max_bytes` (capped at 24 KiB), then 160 px; no metadata. Fails when nothing fits. |
| `check_preview` | function | `(bytes: Vec<u8>) -> Result<Rgba, CoreError>` | Decodes an RGB or grayscale JPEG preview or thumbnail from another user after size and header checks, with a bounded JPEG decoder independent of the encoder; unsupported pixel formats and decoder panics become `CoreError`. `Rgba` is `{ width, height, pixels }`. |
| `strip_metadata` | function | `(bytes: Vec<u8>) -> Result<Vec<u8>, CoreError>` | A JPEG or PNG without metadata before upload: keeps only image data and colour (JFIF, ICC, Adobe; PNG colour chunks), cuts anything after the end marker. A JPEG with an EXIF orientation is turned upright and re-encoded at quality 90; a PNG with one, and any other format, fails, and the app converts it to JPEG first. |
| `client_traffic` | function | `() -> Option<ClientTraffic>` | `received` and `sent` payload bytes of this process's client swarms, for the macOS status bar; `None` until one starts. |
| `set_network_metered` | function | `(metered: bool)` | The app reports whether its network is metered; metered until it does. Apple reports it from `NWPathMonitor` (expensive or constrained). Android reports the default network at process start and from `ConnectivityManager` capability/loss callbacks; absent or unknown capabilities remain metered. This subscription also covers background mail. Instrumented tests can pin the report with `NetworkType.overrideForTest`, restoring it with `null` afterward. Full peer-photo auto-download reads this report. |
| `set_test_node_id_pow_bits` | function | `(bits: u32)` | Tests only: lowers the node-ID proof of work this process requires of nodes (22 bits by default) so a test network's cheap node IDs serve search; the Android search tests set 8 for the emulator runner's node. |
| `set_test_profile_pow_bits` | function | `(bits: u32)` | Tests only: lowers the profile proof of work this process computes and requires of search hits (11 bits by default) for a test network whose node has the same minimum; the iOS and Android UI tests set 4. |
| `load_network` | function | `(dir: String, file: NetworkFile) -> Result<Option<NetworkSettings>, CheckError>` | Reads the identity folder's `Draft` or `Active` bootstrap settings; `None` when never saved. `NetworkSettings` is `{ seeds, name }`. |
| `save_network_draft` | function | `(dir: String, draft: NetworkSettings) -> Result<(), CheckError>` | `save-draft`: stores the draft without connecting. |
| `ConnectionCheck` | Object | `new(dir: String) -> Result<Self, CheckError>` | Checks one identity folder's drafts on its own Tokio runtime; calls block, so the app runs them off the UI thread. |
| | method | `run(draft: NetworkSettings, wait_ms: u32) -> Result<Vec<SeedRow>, CheckError>` | `check-connection` and `retry-connection`: saves the draft, then dials every row from a throwaway key. `SeedRow` is `Connected { peer_id }`, `InvalidAddress`, `InvalidPeerId`, `Duplicate`, `Unreachable { reason }`, `WrongPeer` or `WrongNetwork`, in row order. `CheckError` is `Empty`, `InvalidNetwork { reason }`, `Cancelled`, `NotChecked` or `Failed { reason }`. |
| | method | `cancel()` | `cancel-check`: the running `run` fails with `Cancelled`; the files stay as they were. |
| | method | `activate() -> Result<NetworkSettings, CheckError>` | `activate-connection`: saves the last draft that reached a node as `Active`; `NotChecked` before that. |
| `profile_scales` | function | `() -> Vec<ProfileScale>` | The nine closed public profile scales, with wire field numbers, stable localization keys, order flag and nonzero choices. Android and Apple editors show these choices plus a local “not published” option (zero). |
| `assess_birth_parts` | function | `(year?: u32, month?: u32, day?: u32, today_unix_days: u64) -> Result<BirthAssessment, CoreError>` | Uses the shared profile core to reject impossible partial dates and return inclusive youngest/oldest ages for the caller's UTC day; omitted parts remain absent. |
| `place_name` | function | `(country: String, id: u32, lang: String) -> Option<Place>` | Names a stored profile `place` from the embedded cities15000 set; `lang` `ru` gives Russian names, any other English. `None` outside the set. |
| `place_countries` | function | `() -> Vec<String>` | Place picker: ISO codes of the countries that have places, in code order. |
| `search_places` | function | `(country: String, prefix: String, lang: String, limit: u32) -> Vec<Place>` | Place picker: places of the country whose English or Russian name starts with `prefix` (case-insensitive), largest first; an empty prefix lists the largest. `Place` is `{ id, name, region }`. |
| `BirthAssessment` | Record | `valid: bool`, optional `youngest`, `oldest` (`u32`) | Age bounds are absent without a year or valid age range. A valid yearless month/day stays optional. |
| `ProfileScale`, `ProfileChoice` | Record | Field metadata and `{ code, key }` choices | Copied by value; labels are localized by each client. |
| `PublicProfileDraft` | Record | Fourteen zero-default categorical fields, independently optional birth year, month and day, `country` (ISO code) and `place` (decimal GeoNames ID), each empty when unset, optional `height` in whole centimetres, `about` text, `seeking_genders`, `seeking_age_from` and `seeking_age_to`, empty when unset, and the `not_looking` switch | Editable values only; zero publishes no categorical value. Version and proof-work progress stay in the stored Rust `Profile`. |
| `load_profile_draft` | function | `(dir: String, key: IdentityKey) -> Result<Option<PublicProfileDraft>, CoreError>` | Reads the identity-bound `client.db` draft without creating a database when absent; an identity mismatch fails. The Android and Apple editors and previews use it. |
| `save_profile_draft` | function | `(dir: String, key: IdentityKey, draft: PublicProfileDraft) -> Result<(), CoreError>` | Validates scales, optional birth parts, height 50–272 (`invalidHeight`), `about` of at most 500 characters with no hidden symbols (`invalidAbout`), known seeking genders and partner ages of 18–120 that are not reversed (`invalidSeeking`) and that the country and place are offered by the picker (a place needs its country), then updates only editable fields in `client.db`, retaining version and proof-work progress. `not_looking` drops the seeking fields. The Android and Apple editors use it. |
| `profile_photos` | function | `(dir: String, key: IdentityKey) -> Result<Vec<Vec<u8>>, CoreError>` | The draft's thumbnail hashes in order, empty without a draft; the first photo is the main one. Read-only convenience; Android's snapshot-bound editor uses `profile_photo_snapshot` for actions. |
| `add_profile_photo` | function | `(dir: String, key: IdentityKey, full: Vec<u8>, thumbnail: Vec<u8>) -> Result<Vec<Vec<u8>>, CoreError>` | Appends a photo the app re-encoded and returns the thumbnail hashes. `corrupted` when a frame header is not a whole JPEG within 2048 px (320 px for the thumbnail); `photoRefused` for a seventh photo or a blob over 1 MiB (24 KiB for the thumbnail). |
| `remove_profile_photo`, `move_profile_photo` | function | `(dir, key, index: u32)`, `(dir, key, from: u32, to: u32)` -> `Result<Vec<Vec<u8>>, CoreError>` | Legacy index-only edits, still awaiting Apple UI migration; Android uses `edit_profile_photos` instead. `photoRefused` for an index out of range. A removed blob stays until the next publish. |
| `ProfilePhotoSnapshot` | Record | `revision: Vec<u8>`, `thumbnails: Vec<Vec<u8>>` | One ordered photo snapshot with an opaque core revision covering original hashes, previews and duplicates. Unrelated fields/proof do not invalidate it. |
| `profile_photo_snapshot` | function | `(dir: String, key: IdentityKey) -> Result<ProfilePhotoSnapshot, CoreError>` | Reads revision and display hashes from the same authoritative draft read. |
| `ProfilePhotoEdit` | Enum | `Remove { index: u32 }`, `Move { from: u32, to: u32 }` | Positions in the expected displayed snapshot. |
| `edit_profile_photos` | function | `(dir: String, key: IdentityKey, expected_revision: Vec<u8>, edit: ProfilePhotoEdit) -> Result<ProfilePhotoSnapshot, ProfilePhotoEditError>` | Verifies the revision and edits inside one core writer transaction. Android uses this for remove/move; Apple and Tauri checked adoption remains planned. |
| `ProfilePhotoEditError` | Error | `Changed`, `Failed { reason: String }` | Changed is typed: reload the display without retrying the stale action. Failed retains diagnostic detail for invalid position/storage failures. |
| `profile_photo` | function | `(dir: String, key: IdentityKey, hash: Vec<u8>) -> Result<Vec<u8>, CoreError>` | One stored blob of the owner's photos, or `notFound`. |
| `profile_publish_block` | function | `(dir: String, key: IdentityKey, attested: bool) -> Result<Option<String>, CoreError>` | Read-only shared publish gate at current UTC day. `None` permits the attempt; `notAttested`, `invalidBirthDate` or `underage` explains a block. An empty profile is permitted with fresh 18+ attestation. The Apple and Android publish UIs use it. |
| `ProfileSolve` | Object | `new(dir: String, key: IdentityKey) -> Result<Self, CoreError>` | A per-attempt proof-work handle for the identity-bound draft. Create a new handle for retry after cancellation. |
| | method | `run_step(attested: bool) -> Result<ProfileWorkStep, CoreError>` | Rechecks the fresh 18+ gate and current UTC day, tries eight nonces outside writer ownership, then atomically merges proof/cursor progress in `client.db` without replacing current fields/photos. Pending nonces live in per-target versioned settings, separate from existing valid proofs. Returns solved/cancelled flags and the next nonce; both flags false means call again off the UI thread. The required proof is 11 bits. |
| | method | `cancel()` | Marks the handle cancelled; an in-flight short step saves before stopping. A cancelled handle stays cancelled. |
| `ProfilePublisher` | Object | `new(dir: String) -> Result<Self, CoreError>` | Prepares a private runtime for an existing identity folder; native calls run off the UI thread. |
| | method | `check_connection() -> Result<(), CoreError>` | Dials only the identity's activated network, trying each bootstrap row's TCP and QUIC addresses. A draft-only or empty active network returns `notConnected` without changing `client.db`. |
| | method | `publish(key: IdentityKey, attested: bool) -> Result<ProfilePublished, CoreError>` | Rechecks the fresh 18+ gate, requires the 11-bit proof, connects through active settings and delegates to shared `publish::publish`. It lists no device and publishes no key packages, so the clients publish through `ChatSession.publish_profile`, or nobody could write to them. Only verified signed readback returns a public draft and version; failures preserve the saved draft. Swift `RustBridge.publishProfile` and Kotlin `RustBridge.withKey` reopen and check the selected Peer ID before this call. The Apple and Android publish UIs use it. |
| | method | `hide(key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Connects through active settings, then delegates to shared `publish::hide` and sends what is due. Without a node it returns `notConnected` and changes nothing. The Android hide switch and the Apple hide button use it. |
| | method | `withdraw(key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Before an identity is removed, connects through active settings, then delegates to shared `publish::withdraw` and sends what is due. Without a node it fails and changes nothing, so the caller offers Retry or Remove anyway. |
| `ProfilePublished` | Record | `draft: PublicProfileDraft`, `version: u64`, `current_draft: PublicProfileDraft`, `current_version: u64`, `hidden: bool`, `pending: bool` | `draft` and `version` remain the exact verified signed readback. After follow-up network work, the core reads current persisted fields, version and visibility from one snapshot; `pending` compares content with readback, ignoring version-only changes. Android refreshes its existing editor from `current_draft` only when no unsaved input or save is in flight, and accepts visibility updates only with a newer version. Its published preview uses the exact readback. |
| `profile_visibility` | function | `(dir: String, key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Reads the last used version and the hidden flag without creating a database when absent. The Android hide switch and the Apple hide button use it. |
| `ProfileVisibility` | Record | `version: u64`, `hidden: bool` | Version zero means never published or hidden. |
| `ProfileSearcher` | Object | `new(dir: String) -> Result<Self, CoreError>` | Prepares a private runtime and an in-memory verified profile cache for an existing identity folder; native calls run off the UI thread. Android holds one per open identity for search. |
| | method | `fetch_peer_photos(hashes: Vec<Vec<u8>>, full: bool) -> Result<(), CoreError>` | Fetches the photos the in-memory cache (256 thumbnails, 8 full images, least recently used out) lacks over one connection per seed row. A blob over the size bounds, not a whole JPEG, or over 320 px (thumbnail) or 64 megapixels (full) is not kept; a single blob's failure is no error. Errs `notConnected`. The cache lives as long as the searcher. |
| | method | `peer_photo(hash: Vec<u8>) -> Result<Vec<u8>, CoreError>` | A cached photo's bytes, or `notFound`. Decode a thumbnail with `check_preview`. |
| | method | `profiles(peer_ids: Vec<String>) -> Result<Vec<FoundProfile>, CoreError>` | As `ChatSession::profiles`, and keeps each verified profile in the searcher's cache (an older version never replaces a newer one) for `contact_names`. |
| | method | `contact_names(key: IdentityKey, peer_ids: Vec<String>) -> Result<Vec<ContactNameView>, CoreError>` | Local, in order, by the desktop rule: a local name, else the cached profile's public name with its mark, else none. `ContactNameView` is `{ peer_id, name, verified: Option<bool>, photo }`: `name` is empty without a name, `verified` is absent for a local name, and `photo` is the first thumbnail hash, empty without a cached profile or photo. A public name and photo return after a restart with the next search or `profiles`. |
| | method | `search(key: IdentityKey, conditions: Vec<SearchCondition>, limit: u32) -> Result<ProfileSearchResult, CoreError>` | Connects through active settings, asks up to 3 search providers for their `NodeInfo` and runs shared `search::search` with the 11-bit proof minimum, skipping blocked and passed peers. No conditions asks for a sample; `limit` 0 means 50 hits per node. Every call connects again, so retry is a new call; no answering active network fails with `notConnected`. |
| `peer_photo_auto` | function | `(dir: String, key: IdentityKey) -> Result<bool, CoreError>` | Whether full peer photos load without a tap, from the chat originals' setting and the platform's metered report. |
| `SearchCondition` | Record | `field: u32`, `op: SearchOp` (`Eq`, `In`, `Range`), `ints: Vec<i64>`, `strings: Vec<String>` | One filter on a field number from `offered`. |
| `search_mutual_conditions` | Function | `(draft: PublicProfileDraft) -> Result<Vec<SearchCondition>, CoreError>` | The conditions of a mutual search on today's date ([ADR 0043](../decisions/0043-seeking-and-mutual-search.md)). Empty when the draft states no known gender or an exact adult age; the client then does not offer the option. |
| `ProfileSearchResult` | Record | `offered: Vec<u32>`, `profiles: Vec<FoundProfile>`, `partial: bool`, `failures: Vec<ProfileSearchFailure>` | `offered` lists the fields every reached node filters on and the client checks. `FoundProfile` is `{ peer_id, draft: PublicProfileDraft, photos, full_photos, version }`, verified by the shared core; `photos` holds each photo's thumbnail hash in order and `full_photos` its full image hash, empty where a photo has none, for `fetch_peer_photos`. `partial` is set when a provider was dropped or a node's coverage may be incomplete. `ProfileSearchFailure` is `NoNodes`, `Unavailable`, `Timeout`, `RateLimited { retry_after_ms }`, `Puzzle` or `Invalid`, one per dropped provider or failed request. |
| `ProfileService` | Object | `new()` | Stub; native editors currently save local drafts, while publication and the 18+ gate remain planned. The [client screen contract](../design/v0.0.1-client-screens.md#profile-draft-and-publication) governs their future integration. |
| | method | `get_profile() -> Result<UserProfile, CoreError>` | **Stub.** Returns `placeholder`, no birth-date parts, and `verified = false`. |
| | method | `update_field(field: String, value: String) -> Result<(), CoreError>` | **Stub.** No-op. |
| `UserProfile` | Record | `user_id: String`, optional `birth_year`, `birth_month`, `birth_day` (`u32`), `verified: bool` | Copied by value. |

The Apple `ProfileRustDraftStore` adapter maps the generated record to local editor values and
imports an existing JSON draft only when no Rust draft exists. JSON remains available for retry.
The editor and preview now use this adapter on a background task, reopening the selected key and
checking its Peer ID before touching `client.db`.

Not exported: `start`/`stop` video, `add_ice_candidate`, `local_ice_candidates`, sync, discovery,
or app lifecycle functions.

The first-run connection surface (`load_network` to `ConnectionCheck`) follows the
[typed settings and state contract](../design/v0.0.1-client-screens.md#connection-contract-planned);
it checks seeds but keeps no connection open. Platform UI must not treat the `PeerConnection`
label object as a working network connection.

### Native symbols

The library's C ABI is UniFFI-internal (`nm -D libdyapp_ffi.so`):
`uniffi_dyapp_ffi_fn_constructor_<object>_new`,
`uniffi_dyapp_ffi_fn_method_<object>_<method>`,
`uniffi_dyapp_ffi_fn_free_<object>`,
`uniffi_dyapp_ffi_fn_func_<function>`, plus checksum and
`RustBuffer` helpers. They take `RustBuffer`/`RustCallStatus` arguments and are
meant to be called only by UniFFI-generated code. **There is no hand-callable
C API** (no `dyapp_client_new`, `dyapp_send_message`, …); a C/.NET
consumer would need a separate `extern "C"` layer, which does not exist.

## Error model

Every fallible export returns `Result<T, CoreError>`. `CoreError` is a
`#[uniffi(flat_error)]` enum with one variant, `Failed(String)`, carrying
free-form text (`"Session not initialized"`, `"Failed to create runtime: …"`, or
`{:?}` of the internal video error), so messages may echo internal state. The
Kotlin bindings throw it as `CoreException`, the Swift bindings as `CoreError`. Generation commands are in
[FFI bindings](../development/ffi-bindings.md).

The app's `KeyStore` throws `WrapError::Failed { reason }` instead (Kotlin
`WrapException.Failed`, Swift `WrapError.failed`): a flat error cannot be lifted
from foreign code. Rust turns it, and anything else the app throws, into
`CoreError::Failed`.

## Ownership and threading

- **Objects** (`PeerConnection`, `MessageService`, `VideoSession`,
  `ProfileService`, `IdentityKey`, `MlsDevice`, `MlsGroup`) are `Arc`-backed handles; the native side owns a reference
  and the matching `free` symbol releases it. **Records** (`UserProfile`, `MlsNewGroup`) are copied across the boundary.
- All exports are **synchronous**. `VideoSession` owns its own multi-threaded
  Tokio runtime and every method calls `block_on`, so it blocks the calling
  thread for the whole WebRTC operation — never call it from a UI thread. Each
  `VideoSession` object spawns a separate runtime.
- `VideoSession` methods other than `create_offer` return
  `"Session not initialized"` until `create_offer` has succeeded.
- `KeyStore` is the only callback: Rust calls the app's implementation
  synchronously on the calling thread. No async exports exist.

## Security boundary

The boundary is **not** secure today; see
[Encryption & Security Status](../security/encryption.md).

- The app-lock PIN crosses FFI as plaintext in a UniFFI string. Rust zeroizes its owned copy after each hash or verify call and never logs it, but UI and UniFFI temporary copies are platform-owned. The app must clear its input, protect the 49-byte verifier record, and enforce the five-attempt device-credential gate; the FFI helper does not store state.
- **Plaintext crosses FFI by necessity.** The UI owns what the user types and
  what is displayed, so message text and profile values always pass in
  plaintext (`send_message(peer_id, text)`, `update_field`). The target is that
  Rust encrypts before anything leaves the device, not that plaintext never
  crosses FFI.
- **Secret keys cross FFI only into the keystore.** `IdentityKey` keeps the
  secret in Rust; it reaches native code only as the argument of
  `KeyStore.wrap` and the result of `KeyStore.unwrap`, never as a string.
  Rust wipes its own copies; the FFI buffers that carry the secret are freed,
  not wiped. `MlsDevice` handles its state key the same way and returns MLS state only
  sealed.
- Errors are strings and may echo internal state; do not show them to users
  or send them to telemetry unfiltered.

UniFFI checks types at the boundary, but the generated code is not "zero-cost"
(arguments are serialised through `RustBuffer`), and memory safety still
depends on the native side not using a handle after freeing it.

## Design rules (target)

- Business logic in Rust; platform code is UI and event handling. Platform code may hold layout and
  rendering, input, OS permissions, local file pickers, notification registration, OS settings
  and accessibility. It must not encrypt or decrypt, generate keys, sign or verify records, open
  network connections, discover peers, queue messages, negotiate video codecs or run database
  queries: those stay in the Rust crates.
- Cross the boundary with primitives, strings, records and opaque objects; no
  Protobuf messages or raw key bytes.
- Signed records are parsed and verified only in Rust; Dart never parses signed bytes.
- Typed `uniffi::Error` enums with per-case variants instead of the single `CoreError::Failed`.
- Blocking calls are acceptable for the prototype; if push-style updates are
  needed, prefer polling (`poll_new_messages`) over foreign callbacks.
- Status metrics follow the same rule: the UI polls a record of traffic in and
  out since start (all `WireBytes` slots summed) and the store's bytes used, as
  the [desktop storage and status](../design/v0.0.1-client-screens.md#desktop-storage-and-status)
  table defines; an absent value is `None`, never zero.

## Testing

`cargo test -p dyapp-ffi` wraps and unwraps an `IdentityKey` through a test `KeyStore` and
checks that keystore errors reach the caller; a `ConnectionCheck` saves its draft, activates
only after reaching an in-process node and is cancelled mid-check; two `MlsDevice`s exchange
a message across a sealed restart, and a wrong state key or a malformed profile fails.
App-lock tests cover random salt, a correct and wrong PIN, invalid PIN and malformed verifier records. Preview tests size a 4000 × 3000 photo and noise, and feed `check_preview` a hostile corpus
and 10 000 seeded mutations of a valid preview without a panic. The Android UI test
(`AppUITests.kt`) checks that the stored identity keeps its peer ID when the activity is
recreated; the iOS and macOS UI tests (`LaunchTests.swift`) check that it survives an app
relaunch.
