# FFI Contract

> ⚠️ **Prototype.** This page is the single source of truth for the Rust ↔ native
> boundary, checked against `rust/ffi/src/lib.rs` (UniFFI 0.32.2). Kotlin bindings are
> generated during the Android build, Swift bindings by `tools/scripts/build-apple-core.sh`
> ([FFI bindings](../development/ffi-bindings.md)).

## Which source is the contract

| Source | Status |
|--------|--------|
| `rust/ffi/src/lib.rs` + `rust/ffi/src/dyapp.udl` (crate `dyapp-ffi`) | **The contract.** Workspace member; compiles to `libdyapp_ffi.{so,a}`. Interface is declared with proc macros; the UDL only supplies an empty `namespace dyapp {}` for the UDL scaffolding. |
| `clients/android/app/src/main/kotlin/com/dyapp/RustBridge.kt` | Creates or opens the active identity slot and shows its peer ID. |
| `rust/ffi/src/android.rs` | Android only: the JNI export `DYApplication.nativeInit` that gives the DNS resolver the app `Context` ([ADR 0032](../decisions/0032-android-context-for-dns.md)). |
| `clients/android/app/src/main/kotlin/com/dyapp/Slots.kt` | Identity slots under `noBackupFilesDir`: create, open, recovery states and removal; see [Android identity storage](../development/android-identity-storage.md). |
| `clients/android/app/src/main/kotlin/com/dyapp/PlatformKeyStore.kt` | The app's `KeyStore`: AES-256-GCM with a non-exportable Android Keystore key, one alias per identity slot. |
| `clients/ios/DYApp/RustBridge.swift` | Creates or opens the stored `IdentityKey` and shows its peer ID; shared by the iOS and macOS apps. |
| `clients/ios/DYApp/PlatformKeyStore.swift` | The Apple apps' `KeyStore`: CryptoKit AES-256-GCM with a random key in the Keychain, this device only. |

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
| | method | `receive() -> Result<u32, CoreError>` | Over the active network: resends the staged writes that are due, applies this device's mailbox and refills tokens when due, then, best effort as on the desktop, keeps the last-resort key package and runs the daily photo keep refresh. Returns how many events changed a chat. Senders' profiles are fetched from the node on each call; an envelope whose owner has none there waits in the mailbox for the next call. |
| | method | `chat_rows() -> Result<Vec<ChatRow>, CoreError>`, `chat_history(peer_id: String) -> Result<ChatHistory, CoreError>` | The local projection, no network. Rows leave out blocked peers; a history for a blocked peer errs `blocked`. Both carry `muted`. In a history, `read` on an own live text says the peer's read receipt covers it (the desktop rule); rows leave it false. |
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
| `make_preview` | function | `(rgba: Vec<u8>, width: u32, height: u32, max_bytes: u32) -> Result<Vec<u8>, CoreError>` | JPEG preview of pixels the app decoded: long side ≤ 320 px, quality 80 down to 40 until it fits `max_bytes` (capped at 24 KiB), then 160 px; no metadata. Fails when nothing fits. |
| `check_preview` | function | `(bytes: Vec<u8>) -> Result<Rgba, CoreError>` | Decodes an RGB or grayscale JPEG preview or thumbnail from another user after size and header checks, with a bounded JPEG decoder independent of the encoder; unsupported pixel formats and decoder panics become `CoreError`. `Rgba` is `{ width, height, pixels }`. |
| `strip_metadata` | function | `(bytes: Vec<u8>) -> Result<Vec<u8>, CoreError>` | A JPEG or PNG without metadata before upload: keeps only image data and colour (JFIF, ICC, Adobe; PNG colour chunks), cuts anything after the end marker. A JPEG with an EXIF orientation is turned upright and re-encoded at quality 90; a PNG with one, and any other format, fails, and the app converts it to JPEG first. |
| `set_network_metered` | function | `(metered: bool)` | The app reports whether its network is metered; metered until it does. Chat auto-download will read it. |
| `set_test_node_id_pow_bits` | function | `(bits: u32)` | Tests only: lowers the node-ID proof of work this process requires of nodes (22 bits by default) so a test network's cheap node IDs serve search; the Android search tests set 8 for the emulator runner's node. |
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
| `profile_photos` | function | `(dir: String, key: IdentityKey) -> Result<Vec<Vec<u8>>, CoreError>` | The draft's thumbnail hashes in order, empty without a draft; the first photo is the main one. The Android editor calls these photo commands under one lock with every other draft write. |
| `add_profile_photo` | function | `(dir: String, key: IdentityKey, full: Vec<u8>, thumbnail: Vec<u8>) -> Result<Vec<Vec<u8>>, CoreError>` | Appends a photo the app re-encoded and returns the thumbnail hashes. `corrupted` when a frame header is not a whole JPEG within 2048 px (320 px for the thumbnail); `photoRefused` for a seventh photo or a blob over 1 MiB (24 KiB for the thumbnail). |
| `remove_profile_photo`, `move_profile_photo` | function | `(dir, key, index: u32)`, `(dir, key, from: u32, to: u32)` -> `Result<Vec<Vec<u8>>, CoreError>` | Edit the draft's order; `photoRefused` for an index out of range. A removed blob stays until the next publish. |
| `profile_photo` | function | `(dir: String, key: IdentityKey, hash: Vec<u8>) -> Result<Vec<u8>, CoreError>` | One stored blob of the owner's photos, or `notFound`. |
| `profile_publish_block` | function | `(dir: String, key: IdentityKey, attested: bool) -> Result<Option<String>, CoreError>` | Read-only shared publish gate at current UTC day. `None` permits the attempt; `notAttested`, `invalidBirthDate` or `underage` explains a block. An empty profile is permitted with fresh 18+ attestation. The Apple and Android publish UIs use it. |
| `ProfileSolve` | Object | `new(dir: String, key: IdentityKey) -> Result<Self, CoreError>` | A per-attempt proof-work handle for the identity-bound draft. Create a new handle for retry after cancellation. |
| | method | `run_step(attested: bool) -> Result<ProfileWorkStep, CoreError>` | Rechecks the fresh 18+ gate and current UTC day, tries eight nonces, and persists the next nonce in `client.db`. Returns solved/cancelled flags and the next nonce; both flags false means call again off the UI thread. The required proof is 11 bits. |
| | method | `cancel()` | Marks the handle cancelled; an in-flight short step saves before stopping. A cancelled handle stays cancelled. |
| `ProfilePublisher` | Object | `new(dir: String) -> Result<Self, CoreError>` | Prepares a private runtime for an existing identity folder; native calls run off the UI thread. |
| | method | `check_connection() -> Result<(), CoreError>` | Dials only the identity's activated network, trying each bootstrap row's TCP and QUIC addresses. A draft-only or empty active network returns `notConnected` without changing `client.db`. |
| | method | `publish(key: IdentityKey, attested: bool) -> Result<ProfilePublished, CoreError>` | Rechecks the fresh 18+ gate, requires the 11-bit proof, connects through active settings and delegates to shared `publish::publish`. Only verified signed readback returns a public draft and version; failures preserve the saved draft. Swift `RustBridge.publishProfile` and Kotlin `RustBridge.withKey` reopen and check the selected Peer ID before this call. The Apple and Android publish UIs use it. |
| | method | `hide(key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Connects through active settings, then delegates to shared `publish::hide` and sends what is due. Without a node it returns `notConnected` and changes nothing. The Android hide switch uses it. |
| | method | `withdraw(key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Before an identity is removed, connects through active settings, then delegates to shared `publish::withdraw` and sends what is due. Without a node it fails and changes nothing, so the caller offers Retry or Remove anyway. |
| `ProfilePublished` | Record | `draft: PublicProfileDraft`, `version: u64` | The signed readback values and version after successful publication. |
| `profile_visibility` | function | `(dir: String, key: IdentityKey) -> Result<ProfileVisibility, CoreError>` | Reads the last used version and the hidden flag without creating a database when absent. The Android hide switch uses it. |
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

- Business logic in Rust; platform code is UI and event handling.
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
