import Foundation

/// Bridge to Rust core via UniFFI
class RustBridge {
  static let shared = RustBridge()

  private init() {}

  /// Initialize the Rust core
  func initialize() throws {
    // TODO: Call Rust init via UniFFI
    // let result = rustInitialize()
  }

  /// Shutdown the Rust core
  func shutdown() throws {
    // TODO: Call Rust shutdown via UniFFI
    // let result = rustShutdown()
  }
}
