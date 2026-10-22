import Foundation

/// Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings.
enum RustBridge {
    // ponytail: Keychain and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /// Peer ID of this install's identity key. The first call creates the key and stores it
    /// wrapped by `PlatformKeyStore` in Application Support, excluded from backups: the
    /// wrapping key never leaves this device, so a restored blob could not be opened anyway.
    static func peerId() throws -> String { try identity().peerId() }

    private static func identity() throws -> IdentityKey {
        // The macOS app is not sandboxed: its own folder, not the user's Application Support.
        let directory = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        ).appendingPathComponent(Bundle.main.bundleIdentifier ?? "com.dyapp")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try excludeFromBackup(directory)
        let file = directory.appendingPathComponent("identity.key")
        if FileManager.default.fileExists(atPath: file.path) {
            let wrapped = try Data(contentsOf: file)
            return try IdentityKey.unwrap(store: PlatformKeyStore.shared, wrapped: wrapped)
        }
        let key = IdentityKey.generate()
        try key.wrap(store: PlatformKeyStore.shared).write(to: file, options: .atomic)
        return key
    }

    /// Everything the app keeps (keys, database, MLS state) lives under `directory`; excluding
    /// the folder keeps it out of iCloud and Time Machine backups.
    private static func excludeFromBackup(_ directory: URL) throws {
        var directory = directory
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try directory.setResourceValues(values)
    }
}
