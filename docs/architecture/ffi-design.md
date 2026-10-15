# FFI Contract

> ⚠️ **Prototype.** This page is the single source of truth for the Rust ↔ native
> boundary, checked against `rust/ffi/src/lib.rs` (UniFFI 0.32.2). Kotlin bindings are
> generated during the Android build, Swift bindings by `scripts/build-apple-core.sh`
> ([FFI bindings](../development/ffi-bindings.md)).

## Which source is the contract

| Source | Status |
|--------|--------|
| `rust/ffi/src/lib.rs` + `rust/ffi/src/dyapp.udl` (crate `dyapp-ffi`) | **The contract.** Workspace member; compiles to `libdyapp_ffi.{so,a}`. Interface is declared with proc macros; the UDL only supplies an empty `namespace dyapp {}` for the UDL scaffolding. |
| `android/app/src/main/kotlin/com/dyapp/RustBridge.kt` | Calls `generatePeerId()` from the generated bindings. |
| `ios/DYApp/RustBridge.swift` | Calls `generatePeerId()` from the generated bindings; shared by the iOS and macOS apps. |

The Android, iOS and macOS apps link the library.

## Exported surface

Authored Rust exports now carry API documentation, including explicit stub behavior.
The authored FFI `missing_docs` budget is zero; generated bindings are tracked separately
by the [staged documentation gate](../development/code-quality.md#public-api-documentation-staged).

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
| `generate_peer_id()` | function | `-> String` | Peer ID (hex SHA-256 of the public key) of a freshly generated `rust/identity` key; the key is dropped. |
| `generate_keypair()` | function | `-> Result<KeyPair, CoreError>` | **Stub.** Returns `{ public_key: "placeholder", secret_key: "placeholder" }`. |
| `KeyPair` | Record | `public_key: String`, `secret_key: String` | Copied by value. |
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
`uniffi_dyapp_ffi_fn_func_generate_keypair`, plus checksum and
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

## Ownership and threading

- **Objects** (`PeerConnection`, `MessageService`, `VideoSession`,
  `ProfileService`) are `Arc`-backed handles; the native side owns a reference
  and the matching `free` symbol releases it. **Records** (`KeyPair`,
  `UserProfile`) are copied across the boundary.
- All exports are **synchronous**. `VideoSession` owns its own multi-threaded
  Tokio runtime and every method calls `block_on`, so it blocks the calling
  thread for the whole WebRTC operation — never call it from a UI thread. Each
  `VideoSession` object spawns a separate runtime.
- `VideoSession` methods other than `create_offer` return
  `"Session not initialized"` until `create_offer` has succeeded.
- No callbacks or async exports exist.

## Security boundary

The boundary is **not** secure today; see
[Encryption & Security Status](../security/encryption.md).

- **Plaintext crosses FFI by necessity.** The UI owns what the user types and
  what is displayed, so message text and profile values always pass in
  plaintext (`send_message(peer_id, text)`, `update_field`). The target is that
  Rust encrypts before anything leaves the device, not that plaintext never
  crosses FFI.
- **Secret keys cross FFI today.** `KeyPair.secret_key` is a public field of a
  record copied into native memory (currently the string `"placeholder"`).
  Once real keys exist this would expose them to native code, logs and crash
  reports. Target: key material stays in Rust behind an opaque handle, and
  `KeyPair` exposes only the public key.
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

`cargo test -p dyapp-ffi` runs no tests: the crate has none. The Android UI test
(`AppUITests.kt`) and the iOS and macOS UI tests (`LaunchTests.swift`) check the
peer ID that the apps get from `generate_peer_id` through the generated bindings.
