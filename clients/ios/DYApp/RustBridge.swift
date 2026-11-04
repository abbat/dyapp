import Foundation

/// A local candidate and its optional reopened Peer ID, safe to pass from background I/O.
struct IdentityInventoryRow: Identifiable, Sendable {
    let candidate: IdentityCandidate
    let peerID: String?
    var id: String { candidate.location.marker }
}

struct PublishedProfileSnapshot: Sendable {
    let version: UInt64
    let draft: ProfileDraft
}

/// Bridge to the Rust core (`rust/ffi`) through its generated UniFFI bindings.
enum RustBridge {
    private static let draftWrites = NSLock()

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

    /// List slots without changing selection or creating a key.
    static func identityInventory() throws -> [IdentityInventoryRow] {
        let store = try identityStore()
        return try store.candidates().map { candidate in
            let peerID: String?
            if candidate.state == .ready {
                peerID = try? store.open(candidate.location).peerId()
            } else {
                peerID = nil
            }
            return IdentityInventoryRow(candidate: candidate, peerID: peerID)
        }
    }

    /// Create and select a fresh UUID slot only after an explicit user action.
    static func createIdentity() throws -> IdentityResolution {
        let store = try identityStore()
        let created = try store.create()
        let location = IdentityLocation.slot(created.slot.id)
        let peerID = try store.select(location)
        return .ready(IdentityCandidate(location: location, state: .ready), peerID)
    }

    /// Select an existing ready slot after its key has been reopened successfully.
    static func selectIdentity(_ location: IdentityLocation) throws -> IdentityResolution {
        let store = try identityStore()
        let peerID = try store.select(location)
        return .ready(IdentityCandidate(location: location, state: .ready), peerID)
    }

    /// Remove only the confirmed UUID slot, then resolve the remaining candidates without selection.
    static func removeIdentity(_ id: UUID, confirming peerID: String) throws -> IdentityResolution {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        let store = try identityStore()
        try store.remove(id, confirming: peerID)
        return (try? store.resolve()) ?? .recovery(nil, .unavailable)
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

    static func profileSearcher(for candidate: IdentityCandidate) throws -> ProfileSearcher {
        try ProfileSearcher(dir: networkDirectory(root: try directory(), candidate: candidate).path)
    }

    /// One blocking verified sample; a mutual search without a usable saved gender and age says so
    /// rather than searching unfiltered.
    static func searchProfiles(_ request: SearchRequest) throws -> ProfileSearchResult {
        let key = try identityStore().open(request.candidate.location)
        guard key.peerId() == request.peerId else { throw IdentitySlotError.peerIDMismatch }
        var conditions = request.conditions
        if request.mutual {
            let store = ProfileRustDraftStore(root: try directory(), candidate: request.candidate,
                                              peerId: request.peerId, key: key)
            guard let draft = try withDraftLock({ try store.load() }),
                  case let mutual = try searchMutualConditions(draft: try ProfileRustDraftStore.record(from: draft)),
                  !mutual.isEmpty
            else { throw CoreError.Failed(message: "mutual unavailable") }
            conditions += mutual
        }
        let result = try request.searcher.search(key: key, conditions: conditions, employer: request.employer, limit: 0)
        #if DEBUG
        return try withUITestRow(result)
        #else
        return result
        #endif
    }

    #if DEBUG
    /// Native UI tests get one row ahead of the real answer: the local node's sample can be empty.
    private static func withUITestRow(_ result: ProfileSearchResult) throws -> ProfileSearchResult {
        guard ProcessInfo.processInfo.arguments.contains("--ui-test-found-profile") else { return result }
        let draft = ProfileDraft(country: "DE", name: "Test Peer", height: 180)
        let found = FoundProfile(peerId: "12D3KooWUITestFoundProfile", draft: try ProfileRustDraftStore.record(from: draft),
                                 photos: [], fullPhotos: [], version: 1)
        return ProfileSearchResult(offered: result.offered, profiles: [found] + result.profiles,
                                   partial: result.partial, failures: result.failures)
    }
    #endif

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

    /// A step or save writes back the draft it read, and a publish drops the photo blobs it did not
    /// publish, so every draft write holds this one lock or a concurrent change is lost.
    static func withDraftLock<Value>(_ write: () throws -> Value) rethrows -> Value {
        draftWrites.lock()
        defer { draftWrites.unlock() }
        return try write()
    }

    /// Reopen the selected key and load its Rust profile draft, importing old JSON once if needed.
    static func loadProfileDraft(for candidate: IdentityCandidate, peerId: String) throws -> ProfileDraft? {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let store = ProfileRustDraftStore(root: try directory(), candidate: candidate, peerId: peerId, key: key)
        return try withDraftLock { try store.load() }
    }

    /// Persist exact optional profile fields to this identity's Rust client database.
    static func storeProfileDraft(for candidate: IdentityCandidate, peerId: String,
                                  draft: ProfileDraft) throws {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let store = ProfileRustDraftStore(root: try directory(), candidate: candidate, peerId: peerId, key: key)
        try withDraftLock { try store.save(draft) }
    }

    /// Publish only the selected identity's solved draft through its active network, off the UI thread.
    static func publishProfile(for candidate: IdentityCandidate, peerId: String,
                               attested: Bool) throws -> PublishedProfileSnapshot {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        #if DEBUG
        // Native UI tests see a failed publish while their local node keeps running.
        if ProcessInfo.processInfo.arguments.contains("--ui-test-fail-publish") {
            throw CoreError.Failed(message: "notConnected")
        }
        #endif
        let result = try withDraftLock { try ProfilePublisher(dir: dir.path).publish(key: key, attested: attested) }
        return PublishedProfileSnapshot(version: result.version,
                                        draft: ProfileRustDraftStore.values(from: result.draft))
    }

    /// Whether this identity has published and is hidden from search now.
    static func visibility(for candidate: IdentityCandidate, peerId: String) throws -> ProfileVisibility {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        return try profileVisibility(dir: dir.path, key: key)
    }

    /// Take the profile out of search; without a node this throws and changes nothing.
    static func hideProfile(for candidate: IdentityCandidate, peerId: String) throws -> ProfileVisibility {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        return try withDraftLock { try ProfilePublisher(dir: dir.path).hide(key: key) }
    }

    /// Read-only shared eligibility for the saved identity draft, with fresh 18+ attestation.
    static func publicationBlock(for candidate: IdentityCandidate, peerId: String,
                                 attested: Bool) throws -> String? {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        return try profilePublishBlock(dir: dir.path, key: key, attested: attested)
    }

    /// A new cancellable proof handle for this selected identity; a retry creates a new handle.
    static func profileSolveSession(for candidate: IdentityCandidate, peerId: String) throws -> ProfileSolveSession {
        let key = try identityStore().open(candidate.location)
        guard key.peerId() == peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = networkDirectory(root: try directory(), candidate: candidate)
        return ProfileSolveSession(solver: try ProfileSolve(dir: dir.path, key: key))
    }

    /// The Keychain account wrapping this identity's keys, as `identityStore` opens them.
    static func keyStore(for location: IdentityLocation) -> PlatformKeyStore {
        switch location {
        case .legacy:
            return uiTestSlotID.map(PlatformKeyStore.forSlot) ?? .shared

        case .slot(let id):
            return .forSlot(id)
        }
    }

    static func identityStore() throws -> IdentitySlotStore {
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
