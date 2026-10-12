package com.dyapp

/** Bridge to the Rust core via UniFFI. */
object RustBridge {
  init {
    // TODO: Load native Rust library
    // System.loadLibrary("dyapp_core")
  }

  /** Initializes the Rust core. */
  fun initialize() {
    // TODO: Call Rust init via UniFFI
    // rustInitialize()
  }

  /** Shuts the Rust core down. */
  fun shutdown() {
    // TODO: Call Rust shutdown via UniFFI
    // rustShutdown()
  }
}
