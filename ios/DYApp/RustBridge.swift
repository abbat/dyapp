import Foundation

/// Bridge to Rust core via UniFFI
class RustBridge {
  static let shared = RustBridge()

  private init() {}

  /// Initialize the Rust core
  func initialize() throws {
    // Pending the UniFFI bindings: call Rust init
    // let result = rustInitialize()
  }

  /// Shutdown the Rust core
  func shutdown() throws {
    // Pending the UniFFI bindings: call Rust shutdown
    // let result = rustShutdown()
  }
}
