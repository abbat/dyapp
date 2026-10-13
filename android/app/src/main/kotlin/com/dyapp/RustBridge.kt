package com.dyapp

/** Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings. */
object RustBridge {
    /** Peer ID of a key generated per call; the key is not stored. */
    fun peerId(): String = uniffi.dyapp.generatePeerId()
}
