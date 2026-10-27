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
| `clients/android/app/src/main/kotlin/com/dyapp/Slots.kt` | Identity slots under `noBackupFilesDir`: create, open, recovery states and removal; see [Android identity storage](../development/android-identity-storage.md). |
| `clients/android/app/src/main/kotlin/com/dyapp/PlatformKeyStore.kt` | The app's `KeyStore`: AES-256-GCM with a non-exportable Android Keystore key, one alias per identity slot. |
| `clients/ios/DYApp/RustBridge.swift` | Creates or opens the stored `IdentityKey` and shows its peer ID; shared by the iOS and macOS apps. |
| `clients/ios/DYApp/PlatformKeyStore.swift` | The Apple apps' `KeyStore`: CryptoKit AES-256-GCM with a random key in the Keychain, this device only. |

The Android, iOS and macOS apps link the library. Linux and Windows call
`dyapp-identity` directly; their [storage choice and capability checks](../development/desktop-identity-storage.md)
are implemented in the Tauri shell, outside this UniFFI contract.

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
| `MlsNewGroup` | Record | `group: MlsGroup`, `welcome: Vec<u8>` | The new group and the Welcome for the added devices. |
| `MlsGroup` | Object | `id()`, `members()`, `encrypt(device, plaintext)`, `process(device, message, profiles) -> Result<MlsReceived, CoreError>`, `remove_revoked(device, profiles)`, `export_secret(device, label, context, length)` | One group; each call takes its `MlsDevice`. `MlsReceived` is `Message { sender, plaintext }` or `Commit`. |
| `hash_app_pin` | function | `(pin: String) -> Result<Vec<u8>, CoreError>` | Six-to-64 ASCII decimal digits to a 49-byte record: format 1, random 16-byte salt, 32-byte Argon2id tag (64 MiB, three passes, four lanes). Store the record in device-only protected storage. |
| `verify_app_pin` | function | `(pin: String, record: Vec<u8>) -> Result<bool, CoreError>` | Constant-time tag comparison; `false` for a wrong PIN, error for invalid PIN/record or KDF failure. The platform counts failed attempts, including errors. Both calls block and belong off the UI thread. |
| `make_preview` | function | `(rgba: Vec<u8>, width: u32, height: u32, max_bytes: u32) -> Result<Vec<u8>, CoreError>` | JPEG preview of pixels the app decoded: long side ≤ 320 px, quality 80 down to 40 until it fits `max_bytes` (capped at 24 KiB), then 160 px; no metadata. Fails when nothing fits. |
| `check_preview` | function | `(bytes: Vec<u8>) -> Result<Rgba, CoreError>` | Decodes an RGB or grayscale JPEG preview or thumbnail from another user after size and header checks, with a bounded JPEG decoder independent of the encoder; unsupported pixel formats and decoder panics become `CoreError`. `Rgba` is `{ width, height, pixels }`. |
| `strip_metadata` | function | `(bytes: Vec<u8>) -> Result<Vec<u8>, CoreError>` | A JPEG or PNG without metadata before upload: keeps only image data and colour (JFIF, ICC, Adobe; PNG colour chunks), cuts anything after the end marker. A JPEG with an EXIF orientation is turned upright and re-encoded at quality 90; a PNG with one, and any other format, fails, and the app converts it to JPEG first. |
| `set_network_metered` | function | `(metered: bool)` | The app reports whether its network is metered; metered until it does. Chat auto-download will read it. |
| `load_network` | function | `(dir: String, file: NetworkFile) -> Result<Option<NetworkSettings>, CheckError>` | Reads the identity folder's `Draft` or `Active` bootstrap settings; `None` when never saved. `NetworkSettings` is `{ seeds, name }`. |
| `save_network_draft` | function | `(dir: String, draft: NetworkSettings) -> Result<(), CheckError>` | `save-draft`: stores the draft without connecting. |
| `ConnectionCheck` | Object | `new(dir: String) -> Result<Self, CheckError>` | Checks one identity folder's drafts on its own Tokio runtime; calls block, so the app runs them off the UI thread. |
| | method | `run(draft: NetworkSettings, wait_ms: u32) -> Result<Vec<SeedRow>, CheckError>` | `check-connection` and `retry-connection`: saves the draft, then dials every row from a throwaway key. `SeedRow` is `Connected { peer_id }`, `InvalidAddress`, `InvalidPeerId`, `Duplicate`, `Unreachable { reason }`, `WrongPeer` or `WrongNetwork`, in row order. `CheckError` is `Empty`, `InvalidNetwork { reason }`, `Cancelled`, `NotChecked` or `Failed { reason }`. |
| | method | `cancel()` | `cancel-check`: the running `run` fails with `Cancelled`; the files stay as they were. |
| | method | `activate() -> Result<NetworkSettings, CheckError>` | `activate-connection`: saves the last draft that reached a node as `Active`; `NotChecked` before that. |
| `profile_scales` | function | `() -> Vec<ProfileScale>` | The nine closed public profile scales, with wire field numbers, stable localization keys, order flag and nonzero choices. Android and Apple editors show these choices plus a local “not published” option (zero). |
| `assess_birth_parts` | function | `(year?: u32, month?: u32, day?: u32, today_unix_days: u64) -> Result<BirthAssessment, CoreError>` | Uses the shared profile core to reject impossible partial dates and return inclusive youngest/oldest ages for the caller's UTC day; omitted parts remain absent. |
| `BirthAssessment` | Record | `valid: bool`, optional `youngest`, `oldest` (`u32`) | Age bounds are absent without a year or valid age range. A valid yearless month/day stays optional. |
| `ProfileScale`, `ProfileChoice` | Record | Field metadata and `{ code, key }` choices | Copied by value; labels are localized by each client. |
| `ProfileService` | Object | `new()` | Stub; native editors currently save local drafts, while publication and the 18+ gate remain planned. The [client screen contract](../design/v0.0.1-client-screens.md#profile-draft-and-publication) governs their future integration. |
| | method | `get_profile() -> Result<UserProfile, CoreError>` | **Stub.** Returns `placeholder`, no birth-date parts, and `verified = false`. |
| | method | `update_field(field: String, value: String) -> Result<(), CoreError>` | **Stub.** No-op. |
| `UserProfile` | Record | `user_id: String`, optional `birth_year`, `birth_month`, `birth_day` (`u32`), `verified: bool` | Copied by value. |

Not exported: `start`/`stop` video, `add_ice_candidate`, `local_ice_candidates`, sync, discovery,
search, or app lifecycle functions.

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
