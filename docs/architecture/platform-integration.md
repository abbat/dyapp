# Platform Integration Architecture

## Overview

All platforms use **UniFFI-generated FFI bindings** from the Rust core. Each platform has a **minimal wrapper** with zero business logic — all real work happens in Rust.

**Key principle:** Platform code = UI + event handling only. Business logic stays in Rust.

## FFI Boundary

The exported surface, error model, ownership and threading are defined in
[FFI Contract](ffi-design.md). Today no platform links the Rust library, and no
Swift/Kotlin bindings can be generated (UniFFI 0.24.3 rejects the
`Result<T, String>` errors). No wrapper code exists.

The code samples and directory trees below show the **target** wrapper API and
layout. None of it exists yet.

## iOS (Swift)

### Setup

Not available yet: bindings cannot be generated and no XCFramework is built.
The target pipeline (native library → bindings → XCFramework) is in
[FFI bindings](../development/ffi-bindings.md#pipeline-target).

### Integration (target sketch)

```swift
// Module name not final: no bindings are generated yet.
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
    └── DYappFFI.xcframework    # target; not generated yet
```

**No UIKit/SwiftUI in the wrapper** — view controllers call client methods.

## Android (Kotlin)

### Setup

Not available yet: bindings cannot be generated and no AAR is built. See
[FFI bindings](../development/ffi-bindings.md#pipeline-target).

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

Same pipeline as iOS, with macOS slices in the XCFramework. Not available yet.

### Integration

Same as iOS, but with AppKit instead of UIKit.

```swift
let client = DYappClient()

// SwiftUI or AppKit coordinates events
// All state lives in Rust client
```

## Windows (C#/.NET)

Planned future platform. Would use:
- **C FFI boundary** (C header + .NET P/Invoke)
- **.NET wrappers** for WPF/MAUI
- **Same business logic** in Rust

## Linux (C)

### Setup

`cargo build -p dyapp-ffi --release` builds `libdyapp_ffi.so`, but
UniFFI has no C generator: a C consumer needs a separate `extern "C"` layer,
which does not exist.

### Integration (target sketch)

`libdyapp_ffi` exports only UniFFI-internal symbols, so a C API needs a
separate `extern "C"` layer.

```c
#include "dyapp.h"

DYappClient* client = dyapp_client_new();

// Send message
const char* msg_id = dyapp_send_message(client, "peer_id", "Hello!");

// Clean up
dyapp_string_free(msg_id);
dyapp_client_free(client);
```

### Structure

```
platform/linux-c/
├── dyapp.h                   # C header
├── dyapp.c                   # Thin wrapper
└── CMakeLists.txt                  # Build
```

**No GTK/Qt in the wrapper** — GUI frameworks call C functions.

## Validation: What Stays in Platform Code

### ✅ Allowed in platform wrapper

- UI layout & rendering (SwiftUI, Jetpack Compose, WPF)
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
- WebRTC (`webrtc` crate, video) and QUIC (`quinn`, messaging): separate stacks, not measured
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
