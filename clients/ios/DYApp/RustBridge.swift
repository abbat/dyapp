import Foundation

/// Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings.
enum RustBridge {
    private static var uiTestSlotID: UUID? {
        guard let argument = ProcessInfo.processInfo.arguments.first(where: {
            $0.hasPrefix("--ui-test-slot-root=")
        }) else { return nil }
        return UUID(uuidString: String(argument.dropFirst("--ui-test-slot-root=".count)))
    }

    // ponytail: Keychain and file I/O on the caller's (main) thread; move off it with the
    // client runtime.
    /// Resolve the saved selection without creating a replacement identity.
    static func identityState() throws -> IdentityResolution {
        try identityStore().resolve()
    }

    /// Create and select a fresh UUID slot only after an explicit user action.
    static func createIdentity() throws -> IdentityResolution {
        let store = try identityStore()
        let created = try store.create()
        try store.select(.slot(created.slot.id))
        return try store.resolve()
    }

    /// Existing folder for this identity; the legacy key stays at the app root.
    static func networkDirectory(root: URL, candidate: IdentityCandidate) -> URL {
        switch candidate.location {
        case .legacy:
            return root

        case .slot(let id):
            return root.appendingPathComponent("profiles", isDirectory: true)
                .appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
        }
    }

    /// Read this identity's draft or active network settings off the UI thread.
    static func loadNetworkSettings(for candidate: IdentityCandidate, file: NetworkFile) throws -> NetworkSettings? {
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        return try loadNetwork(dir: dir.path, file: file)
    }

    /// Persist draft edits beside this identity's key off the UI thread.
    static func storeNetworkDraft(for candidate: IdentityCandidate, draft: NetworkSettings) throws {
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        try saveNetworkDraft(dir: dir.path, draft: draft)
    }

    private static func identityStore() throws -> IdentitySlotStore {
        let root = try directory()
        if let id = uiTestSlotID {
            return IdentitySlotStore(root: root, legacyStore: .forSlot(id))
        }
        return IdentitySlotStore(root: root)
    }

    /// The app's own Application Support folder, excluded from backups.
    static func directory() throws -> URL {
        // The macOS app is not sandboxed: its own folder, not the user's Application Support.
        let directory = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true
        ).appendingPathComponent(Bundle.main.bundleIdentifier ?? "com.dyapp")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try excludeFromBackup(directory)
        // Native UI tests isolate their slots while retaining them across a relaunch.
        if let id = uiTestSlotID {
            let root = directory.appendingPathComponent("ui-tests", isDirectory: true)
                .appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            return root
        }
        return directory
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
