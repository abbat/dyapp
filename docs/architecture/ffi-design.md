# FFI Contract

> ⚠️ **Prototype.** This page is the single source of truth for the Rust ↔ native
> boundary, checked against `rust/ffi/src/lib.rs` (UniFFI 0.32.2). Kotlin bindings are
> generated during the Android build, Swift bindings by `tools/scripts/build-apple-core.sh`
> ([FFI bindings](../development/ffi-bindings.md)).

## Which source is the contract

| Source | Status |
|--------|--------|
| `rust/ffi/src/lib.rs` + `rust/ffi/src/dyapp.udl` (crate `dyapp-ffi`) | **The contract.** Workspace member; compiles to `libdyapp_ffi.{so,a}`. Interface is declared with proc macros; the UDL only supplies an empty `namespace dyapp {}` for the UDL scaffolding. |
| `clients/android/app/src/main/kotlin/com/dyapp/RustBridge.kt` | Creates or opens the stored `IdentityKey` and shows its peer ID. |
| `clients/android/app/src/main/kotlin/com/dyapp/PlatformKeyStore.kt` | The app's `KeyStore`: AES-256-GCM with a non-exportable Android Keystore key. |
| `clients/ios/DYApp/RustBridge.swift` | Creates or opens the stored `IdentityKey` and shows its peer ID; shared by the iOS and macOS apps. |
| `clients/ios/DYApp/PlatformKeyStore.swift` | The Apple apps' `KeyStore`: CryptoKit AES-256-GCM with a random key in the Keychain, this device only. |

The Android, iOS and macOS apps link the library.

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
| `VideoSession` | Object | `new() -> Result<Self, CoreError>` | Creates a dedicated Tokio runtime. No peer ID. The WebRTC session itself is created lazily by `create_offer`. |
| | method | `create_offer() -> Result<String, CoreError>` | Creates the inner session on first call, returns an SDP offer. |
| | method | `mark_offer_sent() -> Result<(), CoreError>` | OfferCreated → OfferSent. |
| | method | `receive_answer(sdp: String) -> Result<(), CoreError>` | Parses and applies the remote answer, then applies stored remote ICE candidates (see [video](video.md)). |
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
| `make_preview` | function | `(rgba: Vec<u8>, width: u32, height: u32, max_bytes: u32) -> Result<Vec<u8>, CoreError>` | JPEG preview of pixels the app decoded: long side ≤ 320 px, quality 80 down to 40 until it fits `max_bytes` (capped at 24 KiB), then 160 px; no metadata. Fails when nothing fits. |
| `check_preview` | function | `(bytes: Vec<u8>) -> Result<Rgba, CoreError>` | Decodes a preview or thumbnail from another user after size and header checks, under decoder limits; a decoder panic becomes `CoreError`. `Rgba` is `{ width, height, pixels }`. |
| `strip_metadata` | function | `(bytes: Vec<u8>) -> Result<Vec<u8>, CoreError>` | A JPEG or PNG without metadata before upload: keeps only image data and colour (JFIF, ICC, Adobe; PNG colour chunks), cuts anything after the end marker. A JPEG with an EXIF orientation is turned upright and re-encoded at quality 90; a PNG with one, and any other format, fails, and the app converts it to JPEG first. |
| `set_network_metered` | function | `(metered: bool)` | The app reports whether its network is metered; metered until it does. Chat auto-download will read it. |
| `ProfileService` | Object | `new()` | Stub. |
| | method | `get_profile() -> Result<UserProfile, CoreError>` | **Stub.** Returns `{ "placeholder", 0, false }`. |
| | method | `update_field(field: String, value: String) -> Result<(), CoreError>` | **Stub.** No-op. |
| `UserProfile` | Record | `user_id: String`, `age: i32`, `verified: bool` | Copied by value. |

Not exported: `start`/`stop` video, `add_ice_candidate`, `mark_connected`, any
callee/`create_answer` path, sync, discovery, search, or app lifecycle functions.

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

## Testing

`cargo test -p dyapp-ffi` wraps and unwraps an `IdentityKey` through a test `KeyStore` and
checks that keystore errors reach the caller; two `MlsDevice`s exchange a message across a
sealed restart, and a wrong state key or a malformed profile fails. Preview tests size a
4000 × 3000 photo and noise, and feed `check_preview` a hostile corpus and 10 000 seeded
mutations of a valid preview without a panic. The Android UI test (`AppUITests.kt`) checks
that the stored identity keeps its peer ID when the activity is recreated; the iOS and macOS
UI tests (`LaunchTests.swift`) check that it survives an app relaunch.
