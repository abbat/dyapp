package com.dyapp

/// Bridge to Rust core via UniFFI
object RustBridge {
  init {
    // TODO: Load native Rust library
    // System.loadLibrary("dyapp_core")
  }

  /// Initialize the Rust core
  fun initialize() {
    // TODO: Call Rust init via UniFFI
    // rustInitialize()
  }

  /// Shutdown the Rust core
  fun shutdown() {
    // TODO: Call Rust shutdown via UniFFI
    // rustShutdown()
  }
}
