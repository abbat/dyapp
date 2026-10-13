/// Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings.
enum RustBridge {
    /// Peer ID of a key generated per call; the key is not stored.
    static func peerId() -> String { generatePeerId() }
}
