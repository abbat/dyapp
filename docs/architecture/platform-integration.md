# Platform Integration Architecture

## Overview

Android and the Apple apps use **UniFFI-generated FFI bindings** from the Rust core; the Tauri desktop apps are Rust and need no bindings. Each platform has a **minimal wrapper** with zero business logic — all real work happens in Rust.

**Key principle:** Platform code = UI + event handling only. Business logic stays in Rust.

## FFI Boundary

The exported surface, error model, ownership and threading are defined in
[FFI Contract](ffi-design.md). The Android app links the Rust library through
generated Kotlin bindings, the iOS and macOS apps through generated Swift bindings.

The code samples and directory trees below show the **target** wrapper API and
layout. Apart from `RustBridge.kt` and `RustBridge.swift` with their `PlatformKeyStore`, none of
it exists yet.

## iOS (Swift)

### Setup

`tools/scripts/build-apple-core.sh` builds `build/apple/DyappFFI.xcframework` and the
Swift bindings; the app shows the peer ID of its stored identity key through
`RustBridge.swift`: the first start creates the key, `PlatformKeyStore.swift` wraps it with
CryptoKit AES-256-GCM under a random key kept in the Keychain
(`kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`), and the blob lives in the app's
Application Support folder, excluded from backups. The macOS app shares both files.
`AppLockStore.swift` keeps the app-lock choice, the five-attempt failure counter and the Rust
Argon2id PIN verifier in one `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` Keychain item,
apart from the identity items, and counts an attempt before verifying it. Keychain items survive
an uninstall, so a fresh install drops the old item when its `app-lock-installed` marker in the
app folder is missing. `LockGate.swift` (iOS only) shows first-launch Skip/Set PIN or the unlock
screen and builds the start screen only once the lock allows it; the KDF runs off the main
thread. While a lock is set it covers the scene whenever it is not active, so the app-switcher
snapshot shows no content, and `AppLockTimer` relocks 60 s after backgrounding or five minutes
after the last touch on the app's windows, on a monotonic clock that counts device sleep.
Debug builds accept `--app-lock-new` and `--app-lock-off` so UI tests start from a known
lock state. See
[FFI bindings](../development/ffi-bindings.md#apple). The sketch below is the target API.

### Integration (target sketch)

```swift
let client = DYappClient()

// Send message (business logic in Rust)
let messageId = try await client.sendMessage(to: "peer_id", text: "Hello!")

// Start video call (all WebRTC in Rust)
let session = try await client.startVideoCall()
```

### Structure

```
platform/ios-swift/
├── Package.swift                    # SPM manifest
├── Sources/
│   └── DYappClient.swift       # Thin wrapper
└── Frameworks/
    └── DyappFFI.xcframework    # built today in build/apple
```

**No UIKit/SwiftUI in the wrapper** — view controllers call client methods.

## Android (Kotlin)

### Setup

The app build compiles `rust/ffi` per ABI and generates the Kotlin bindings; the
app shows the peer ID of its active identity through `RustBridge.kt`: the first
start creates the key, `PlatformKeyStore.kt` wraps each identity with its own AES-256-GCM
key that never leaves the Android Keystore, and the blobs live in `noBackupFilesDir` (no
backups; the wrapping key could not open a restored copy anyway). The start screen can
switch and add identities; see
[Android identity storage](../development/android-identity-storage.md) and
[FFI bindings](../development/ffi-bindings.md#android). The sketch below is the target API.

### Integration (target sketch)

```kotlin
import com.dyapp.client.DYappClient

val client = DYappClient()

// Send message (suspending function delegates to Rust)
val messageId = client.sendMessage(peerId = "peer_id", text = "Hello!")

// Start video call
val session = client.startVideoCall()
```

### Structure

```
platform/android-kotlin/
├── build.gradle.kts                # Gradle build
├── src/main/kotlin/
│   └── DYappClient.kt         # Thin wrapper
└── src/main/jniLibs/
    ├── arm64-v8a/
    │   └── libdyapp_ffi.so
    └── armeabi-v7a/
        └── libdyapp_ffi.so
```

**No Android Framework calls in the wrapper** — activities delegate to client.

## macOS (Swift)

### Setup

Same pipeline as iOS, with the macOS slice of the XCFramework; the app shares
`ContentView.swift` and `RustBridge.swift` with iOS.

### Integration

Same as iOS, but with AppKit instead of UIKit.

```swift
let client = DYappClient()

// SwiftUI or AppKit coordinates events
// All state lives in Rust client
```

## Windows and Linux (Tauri)

`clients/windows/` and `clients/linux/` are Tauri 2 apps with a bundled static HTML page
([empty-app contract](../platforms/empty-app-contract.md)). Being Rust, they can link the core
crates directly, with no FFI layer and no C or .NET wrapper; neither calls the core yet.

## Validation: What Stays in Platform Code

### ✅ Allowed in platform wrapper

- UI layout & rendering (SwiftUI, Jetpack Compose, Tauri HTML)
- Event handling (button taps, text input)
- Platform-specific permissions (camera, microphone, contacts)
- Local file access
- Push notification registration
- Platform settings integration
- Native text input (keyboard handling)
- Accessibility (VoiceOver, TalkBack)

### ❌ Forbidden in platform code

- Encryption/decryption (Rust E2E layer)
- Peer discovery (Rust P2P layer)
- Message queuing (Rust messaging layer)
- Profile signing and versioning (Rust profile layer)
- Video codec negotiation (Rust video layer)
- Database operations (Rust storage layer)
- Network connections (all Rust)
- Crypto key generation (Rust only)

**Reality check:** Platform code should be <20% of total LOC. If >30%, business logic leaked into platform.

## Testing

### Platform Layer Tests

```swift
// iOS test example (target; DYappClient is a stub today)
@MainActor
func testSendMessageFlow() async throws {
    let client = DYappClient()

    let msgId = try await client.sendMessage(to: "alice", text: "Hi!")
    XCTAssertNotNil(msgId)
}
```

Tests verify:
- FFI calls work correctly
- Async/await chains execute
- Error handling surfaces properly
- No crashes from nil dereference

Tests do NOT verify:
- Message ordering (Rust layer)
- Encryption (Rust layer)
- Peer discovery (Rust layer)

## Deployment

### iOS App Store

```
1. Build release binary for iOS targets
2. Package an .xcframework (xcodebuild -create-xcframework; lipo only within one platform)
3. Embed in Xcode project
4. Submit to App Store (Notarization: auto from provided framework)
```

### Google Play

```
1. Build release binaries for Android targets
2. Package into AAR with Gradle
3. Publish to Play Store
```

### Linux Distribution

```
1. Build release binary for x86_64-linux-gnu
2. Package .deb or .rpm
3. Distribute via package manager
```

## Build Automation

### CI/CD Pipeline

```
Commit to main
  ├─ Build Rust core (cargo test)
  │  └─ FFI tests pass
  │
  ├─ Generate all platform bindings
  │  ├─ Swift (iOS/macOS)
  │  ├─ Kotlin (Android)
  │  └─ C (Linux)
  │
  ├─ Build platform wrappers
  │  ├─ iOS (Xcode)
  │  ├─ Android (Gradle)
  │  └─ Linux (CMake)
  │
  └─ Run integration tests
     └─ Multi-platform peer test
```

## Binary Size

Estimates only: no release build of the FFI library has been measured, and no
app links it yet.

| Platform | Binary Size |
|----------|-------------|
| iOS      | ~15 MB     |
| Android  | ~20 MB     |
| macOS    | ~12 MB     |
| Linux    | ~10 MB     |

Breakdown:
- Rust core: 8-10 MB
- WebRTC (`webrtc` crate, video) and libp2p (messaging): separate stacks, not measured
- Crypto (ChaCha, Ed25519): 0.5 MB

## Limitations & Future

**Current limitations:**
- Single platform shown (others are similar structure)
- No UI example (intentional: platform choice is user's)

**Future enhancements:**
- Example native UI (SwiftUI + Jetpack Compose)
- Streaming binary downloads (not embedded in app)
- Hot reload for development (platform-dependent)
- Platform-specific optimizations (hardware acceleration)
