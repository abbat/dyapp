# FFI Contract

> ⚠️ **Prototype.** This page is the single source of truth for the Rust ↔ native
> boundary. Everything here was checked against `rust/ffi/src/lib.rs` and a build
> of `dyapp-ffi` (UniFFI 0.24.3) on 2026-10-06; the crate has since moved to UniFFI 0.32
> (2026-10-07, to drop unmaintained `bincode`/`paste`) and binding generation was not
> re-checked. **No Swift or Kotlin
> bindings can currently be generated** — see [Error model](#error-model).

## Which source is the contract

| Source | Status |
|--------|--------|
| `rust/ffi/src/lib.rs` + `rust/ffi/src/dyapp.udl` (crate `dyapp-ffi`) | **The contract.** Workspace member; compiles to `libdyapp_ffi.{so,a}`. Interface is declared with proc macros; the UDL only supplies an empty `namespace dyapp {}` for the UDL scaffolding. |
| `ios/DYApp/RustBridge.swift` | `initialize()`/`shutdown()` are empty `TODO`s. |

No platform app links the Rust library today.

## Exported surface

All exports, with what they really do. "Stub" means the body ignores its inputs
or returns constants.

| Rust export | Kind | Signature | Behaviour |
|-------------|------|-----------|-----------|
| `PeerConnection` | Object | `new(peer_id: String)`, `get_id() -> String` | Holds the string; no networking. |
| `MessageService` | Object | `new()` | Stub. |
| | method | `send_message(peer_id: String, text: String) -> Result<String, String>` | **Stub.** Drops `text`, returns `"Message queued to <peer_id>"`. Nothing is queued, encrypted or sent. |
| | method | `queue_size() -> i32` | Stub, always `0`. |
| `VideoSession` | Object | `new() -> Result<Self, String>` | Creates a dedicated Tokio runtime. No peer ID. The WebRTC session itself is created lazily by `create_offer`. |
| | method | `create_offer() -> Result<String, String>` | Creates the inner session on first call, returns an SDP offer. |
| | method | `mark_offer_sent() -> Result<(), String>` | OfferCreated → OfferSent. |
| | method | `receive_answer(sdp: String) -> Result<(), String>` | Stores the answer; **not applied** to the peer connection (see [video](video.md)). |
| | method | `close() -> Result<(), String>` | Closes the session. |
| | method | `get_session_id() -> Result<String, String>` | Session UUID. |
| `generate_keypair()` | function | `-> Result<KeyPair, String>` | **Stub.** Returns `{ public_key: "placeholder", secret_key: "placeholder" }`. |
| `KeyPair` | Record | `public_key: String`, `secret_key: String` | Copied by value. |
| `ProfileService` | Object | `new()` | Stub. |
| | method | `get_profile() -> Result<UserProfile, String>` | **Stub.** Returns `{ "placeholder", 0, false }`. |
| | method | `update_field(field: String, value: String) -> Result<(), String>` | **Stub.** No-op. |
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

Every fallible export returns `Result<T, String>`; errors are free-form text
(`"Session not initialized"`, `"Failed to create runtime: …"`, or `{:?}` of the
internal video error).

**This blocks binding generation.** The Rust scaffolding compiles, but
`uniffi-bindgen` 0.24.3 rejected the interface (not re-checked with 0.32):

```text
panicked at uniffi_bindgen-0.24.3/src/interface/function.rs:162:18:
unsupported error type Some(String)
```

(UDL mode with `--lib-file`, both `--language swift` and `kotlin`; library mode
fails earlier on the `dyapp-ffi` vs `dyapp_ffi` package name.)
Until the errors become a `#[derive(uniffi::Error)]` type, there are no Swift
or Kotlin files, so **generated type and method names are unverified**; do not
rely on any name in other docs. Generation commands are in
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
- Typed `uniffi::Error` enums instead of `String` errors.
- Blocking calls are acceptable for the prototype; if push-style updates are
  needed, prefer polling (`poll_new_messages`) over foreign callbacks.

## Testing

`cargo test -p dyapp-ffi` runs no tests: the crate has none. There are no
binding-generation checks or native FFI tests; Swift/Kotlin "bridge" tests
exercise the hand-written stubs above, not Rust.
