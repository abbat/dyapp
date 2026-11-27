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
    private static let identityStores = IdentityStoreCache()

    private static var uiTestSlotID: UUID? {
        guard
            let argument = ProcessInfo.processInfo.arguments.first(where: {
                $0.hasPrefix("--ui-test-slot-root=")
            })
        else { return nil }
        return UUID(uuidString: String(argument.dropFirst("--ui-test-slot-root=".count)))
    }

    /// Resolve the saved selection without creating a replacement identity.
    static func identityState() throws -> IdentityResolution {
        #if DEBUG
            // The local test node carries a cheap ID, and search skips nodes without the full proof of work.
            // The profile proof is lowered with the node's minimum: the full one can outlast a test's wait.
            if uiTestSlotID != nil {
                setTestNodeIdPowBits(bits: 8)
                setTestProfilePowBits(bits: 4)
            }
            try seedUITestIdentity()
        #endif
        return try identityStore().resolve()
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
    static func createIdentity() throws -> IdentityUpdate {
        let store = try identityStore()
        let created = try store.create()
        #if DEBUG
            seedCreatedIdentityFailure(created.update)
        #endif
        return created.update
    }

    /// Select an existing ready slot after its key has been reopened successfully.
    static func selectIdentity(_ location: IdentityLocation) throws -> IdentityUpdate {
        let store = try identityStore()
        return try store.select(location)
    }

    /// Remove only the confirmed UUID slot, then resolve the remaining candidates without selection.
    static func removeIdentity(_ id: UUID, confirming peerID: String) throws -> IdentityUpdate {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        let store = try identityStore()
        let detail = try store.remove(id, confirming: peerID)
        return IdentityUpdate(
            resolution: (try? store.resolve()) ?? .recovery(nil, .unavailable),
            durabilityDetail: detail)
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
            let store = ProfileRustDraftStore(
                root: try directory(), candidate: request.candidate,
                peerId: request.peerId, key: key)
            guard let draft = try withDraftLock({ try store.load() }),
                case let mutual = try searchMutualConditions(
                    draft: try ProfileRustDraftStore.record(from: draft)),
                !mutual.isEmpty
            else { throw CoreError.Failed(message: "mutual unavailable") }
            conditions += mutual
        }
        let result = try request.searcher.search(
            key: key, conditions: conditions, employer: request.employer, limit: 0)
        #if DEBUG
            return try withUITestRow(result)
        #else
            return result
        #endif
    }

    #if DEBUG
        /// Native UI tests get one row ahead of the real answer: the local node's sample can be empty.
        private static func withUITestRow(_ result: ProfileSearchResult) throws
            -> ProfileSearchResult
        {
            guard ProcessInfo.processInfo.arguments.contains("--ui-test-found-profile") else {
                return result
            }
            let draft = ProfileDraft(country: "DE", name: "Test Peer", height: 180)
            let found = FoundProfile(
                peerId: "12D3KooWUITestFoundProfile",
                draft: try ProfileRustDraftStore.record(from: draft),
                photos: [], fullPhotos: [], version: 1)
            return ProfileSearchResult(
                offered: result.offered, profiles: [found] + result.profiles,
                partial: result.partial, failures: result.failures)
        }
    #endif

    /// Read this identity's draft or active network settings off the UI thread.
    static func loadNetworkSettings(
        for candidate: IdentityCandidate, peerId: String, file: NetworkFile
    ) throws -> NetworkSettings? {
        try withIdentity(candidate, peerId: peerId) { dir, _ in
            try loadNetwork(dir: dir, file: file)
        }
    }

    /// Persist draft edits beside this identity's key off the UI thread.
    static func storeNetworkDraft(
        for candidate: IdentityCandidate, peerId: String, draft: NetworkSettings
    ) throws {
        try withIdentity(candidate, peerId: peerId) { dir, _ in
            try saveNetworkDraft(dir: dir, draft: draft)
        }
    }

    /// A step or save writes back the draft it read, and a publish drops the photo blobs it did not
    /// publish, so every draft write holds this one lock or a concurrent change is lost.
    static func withDraftLock<Value>(_ write: () throws -> Value) rethrows -> Value {
        draftWrites.lock()
        defer { draftWrites.unlock() }
        return try write()
    }

    /// Scope nonescaping native work; acquire draft serialization first when it is needed.
    static func withIdentity<Value>(
        _ candidate: IdentityCandidate, peerId: String,
        _ work: (String, IdentityKey) throws -> Value
    ) throws -> Value {
        try identityStore().withIdentity(candidate.location, peerId: peerId, work)
    }

    /// Reopen the selected key and load its Rust profile draft, importing old JSON once if needed.
    static func loadProfileDraft(for candidate: IdentityCandidate, peerId: String) throws
        -> ProfileDraft?
    {
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { _, key in
                try ProfileRustDraftStore(
                    root: directory(), candidate: candidate, peerId: peerId, key: key
                ).load()
            }
        }
    }

    /// Persist exact optional profile fields to this identity's Rust client database.
    static func storeProfileDraft(
        for candidate: IdentityCandidate, peerId: String,
        draft: ProfileDraft
    ) throws {
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { _, key in
                try ProfileRustDraftStore(
                    root: directory(), candidate: candidate, peerId: peerId, key: key
                ).save(draft)
            }
        }
    }

    /// Publish only the selected identity's solved draft through its active network, off the UI thread.
    static func publishProfile(
        for candidate: IdentityCandidate, peerId: String,
        attested: Bool
    ) throws -> PublishedProfileSnapshot {
        #if DEBUG
            // Native UI tests see a failed publish while their local node keeps running.
            if ProcessInfo.processInfo.arguments.contains("--ui-test-fail-publish") {
                throw CoreError.Failed(message: "notConnected")
            }
        #endif
        // Only a chat session lists this device and its key packages; without them nobody can write here.
        let result = try withDraftLock {
            try withChats(ChatIdentity(candidate: candidate, peerId: peerId)) { session in
                try session.publishProfile(attested: attested)
            }
        }
        return PublishedProfileSnapshot(
            version: result.version,
            draft: ProfileRustDraftStore.values(from: result.draft))
    }

    /// Whether this identity has published and is hidden from search now.
    static func visibility(for candidate: IdentityCandidate, peerId: String) throws
        -> ProfileVisibility
    {
        try withIdentity(candidate, peerId: peerId) { dir, key in
            try profileVisibility(dir: dir, key: key)
        }
    }

    /// Take the profile out of search; without a node this throws and changes nothing.
    static func hideProfile(for candidate: IdentityCandidate, peerId: String) throws
        -> ProfileVisibility
    {
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { dir, key in
                try ProfilePublisher(dir: dir).hide(key: key)
            }
        }
    }

    /// Sign the withdrawal before the identity is removed; without a node this throws and changes nothing.
    static func withdrawProfile(for candidate: IdentityCandidate, peerId: String) throws {
        #if DEBUG
            // Native UI tests see no node accepting it while their local node keeps running.
            if ProcessInfo.processInfo.arguments.contains("--ui-test-fail-withdraw") {
                throw CoreError.Failed(message: "notConnected")
            }
        #endif
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { dir, key in
                _ = try ProfilePublisher(dir: dir).withdraw(key: key)
            }
        }
    }

    /// Read-only shared eligibility for the saved identity draft, with fresh 18+ attestation.
    static func publicationBlock(
        for candidate: IdentityCandidate, peerId: String,
        attested: Bool
    ) throws -> String? {
        try withDraftLock {
            try withIdentity(candidate, peerId: peerId) { dir, key in
                try profilePublishBlock(dir: dir, key: key, attested: attested)
            }
        }
    }

    /// A new cancellable proof handle for this selected identity; a retry creates a new handle.
    static func profileSolveSession(for candidate: IdentityCandidate, peerId: String) throws
        -> ProfileSolveSession
    {
        ProfileSolveSession(
            solver: try ProfileSolve.forSlot(
                registry: identityStore().registry, id: candidate.location.marker,
                expectedPeer: peerId))
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
        return identityStores.store(
            root: root, legacyStore: uiTestSlotID.map(PlatformKeyStore.forSlot) ?? .shared)
    }

    /// The app's own Application Support folder, excluded from backups.
    static func directory() throws -> URL {
        // The macOS app is not sandboxed: its own folder, not the user's Application Support.
        let directory = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil,
            create: true
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

#if DEBUG
    extension RustBridge {
        /// Keep DEBUG corruption fixtures on the same IO queue as their explicit creation.
        private static func seedCreatedIdentityFailure(_ update: IdentityUpdate) {
            guard uiTestSlotID != nil, case let .ready(candidate, _) = update.resolution,
                let root = try? directory()
            else { return }
            let slot = networkDirectory(root: root, candidate: candidate)
            if ProcessInfo.processInfo.arguments.contains("--ui-test-corrupt-network-draft") {
                try? Data("invalid network file\n".utf8).write(
                    to: slot.appendingPathComponent("network-draft"), options: .atomic)
            }
            if ProcessInfo.processInfo.arguments.contains("--ui-test-corrupt-identity-key") {
                try? Data("invalid wrapped key".utf8).write(
                    to: slot.appendingPathComponent("identity.key"), options: .atomic)
            }
        }

        /// Seed one identity state into the isolated UI test root only; a relaunch finds it and adds nothing.
        private static func seedUITestIdentity() throws {
            let prefix = "--ui-test-identity-fixture="
            guard uiTestSlotID != nil,
                let argument = ProcessInfo.processInfo.arguments.first(where: {
                    $0.hasPrefix(prefix)
                })
            else { return }
            let store = try identityStore()
            guard try store.candidates().isEmpty else { return }
            let root = try directory()
            let fixture = String(argument.dropFirst(prefix.count))
            switch fixture {
            case "legacy":
                try IdentityKey.generate().wrap(store: keyStore(for: .legacy))
                    .write(to: root.appendingPathComponent("identity.key"), options: .atomic)

            case "missing-legacy-key":
                // Wrapping leaves the legacy Keychain item behind, with no root file to open.
                _ = try IdentityKey.generate().wrap(store: keyStore(for: .legacy))

            case "missing-slot-key", "pending-slot":
                let slot = root.appendingPathComponent("profiles", isDirectory: true)
                    .appendingPathComponent(UUID().uuidString.lowercased(), isDirectory: true)
                try FileManager.default.createDirectory(at: slot, withIntermediateDirectories: true)
                try Data((fixture == "pending-slot" ? "pending\n" : "ready\n").utf8)
                    .write(to: slot.appendingPathComponent("state"), options: .atomic)

            case "malformed-pointer":
                _ = try store.create()
                try Data("not a pointer\n".utf8).write(
                    to: root.appendingPathComponent("active-profile"), options: .atomic)

            default:
                break
            }
        }
    }
#endif
