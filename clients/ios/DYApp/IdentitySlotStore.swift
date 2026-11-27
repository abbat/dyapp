import Foundation

/// Inventoried iOS slot metadata; lifecycle decisions belong to the common registry.
struct IdentitySlot {
    enum State: String, Sendable { case pending, ready, deleting, damaged }
    let id: UUID
    let state: State
}

enum IdentitySlotError: Error {
    case notReady
    case peerIDMismatch
}

/// A UUID slot or the existing root identity, retaining its original Keychain account.
enum IdentityLocation: Equatable, Sendable {
    case legacy
    case slot(UUID)

    var marker: String {
        switch self {
        case .legacy:
            "legacy-primary"

        case .slot(let id):
            id.uuidString.lowercased()
        }
    }

    init(native id: String) throws {
        if id == "legacy-primary" { self = .legacy; return }
        guard let uuid = UUID(uuidString: id), uuid.uuidString.lowercased() == id else {
            throw CoreError.Failed(message: "Invalid native slot identifier")
        }
        self = .slot(uuid)
    }
}

struct IdentityCandidate: Equatable, Sendable {
    let location: IdentityLocation
    let state: IdentitySlot.State
}

enum IdentityRecoveryReason: Equatable, Sendable {
    case missing, damaged, unavailable, invalidPointer, incomplete, deleting

    init(native reason: MobileSlotRecovery) {
        switch reason {
        case .missing:
            self = .missing

        case .unavailable:
            self = .unavailable

        case .incomplete:
            self = .incomplete

        case .deleting:
            self = .deleting

        case .damaged:
            self = .damaged

        case .invalidPointer:
            self = .invalidPointer
        }
    }
}

enum IdentityResolution: Equatable, Sendable {
    case setup
    case picker([IdentityCandidate])
    case ready(IdentityCandidate, String)
    case recovery(IdentityLocation?, IdentityRecoveryReason, peerId: String? = nil)
}

/// A returned projection and independent persistence diagnostic; a warning is not a failed commit.
struct IdentityUpdate: Equatable, Sendable {
    let resolution: IdentityResolution
    let durabilityDetail: String?

    init(resolution: IdentityResolution, durabilityDetail: String? = nil) {
        self.resolution = resolution
        self.durabilityDetail = durabilityDetail
    }

    init(native selected: MobileSlotSelection) throws {
        resolution = .ready(try IdentityCandidate(native: selected.slot), selected.key.peerId())
        durabilityDetail = selected.durabilityDetail
    }
}

extension IdentityCandidate {
    init(native slot: MobileSlot) throws {
        let state: IdentitySlot.State
        switch slot.state {
        case .pending:
            state = .pending

        case .ready:
            state = .ready

        case .deleting:
            state = .deleting

        case .broken:
            state = .damaged
        }
        self.init(location: try IdentityLocation(native: slot.id), state: state)
    }
}

/// One native ownership view per trusted application root, shared by foreground and background work.
final class IdentityStoreCache: @unchecked Sendable {
    private let lock = NSLock()
    private var stores: [String: IdentitySlotStore] = [:]

    func store(root: URL, legacyStore: PlatformKeyStore) -> IdentitySlotStore {
        lock.lock()
        defer { lock.unlock() }
        let path = root.standardizedFileURL.path
        if let existing = stores[path] { return existing }
        let store = IdentitySlotStore(root: root.standardizedFileURL, legacyStore: legacyStore)
        stores[path] = store
        return store
    }
}

/// DTO facade only: native core owns every identity-file read/write, lock and recovery decision.
final class IdentitySlotStore: Sendable {
    let registry: MobileIdentityRegistry

    init(
        root: URL, legacyStore: PlatformKeyStore = .shared,
        keys: (any MobileSlotKeys)? = nil,
        eraseWrappingKey: @escaping @Sendable (UUID) throws -> Void = {
            try PlatformKeyStore.eraseWrappingKey(forSlot: $0)
        }
    ) {
        registry = MobileIdentityRegistry(
            root: root.path, layout: .ios,
            keys: keys ?? NativeSlotKeys(legacy: legacyStore, eraseKey: eraseWrappingKey))
    }

    func slots() throws -> [IdentitySlot] {
        try candidates().compactMap {
            guard case let .slot(id) = $0.location else { return nil }
            return IdentitySlot(id: id, state: $0.state)
        }
    }

    func candidates() throws -> [IdentityCandidate] {
        try registry.list().map { try IdentityCandidate(native: $0) }
    }

    /// Read-only native guidance; removal independently checks the confirmation again.
    func removalReview(_ location: IdentityLocation, peerId: String) throws
        -> MobileIdentityRemovalReview
    {
        try registry.removalReview(id: location.marker, expectedPeer: peerId)
    }

    func resolve() throws -> IdentityResolution {
        switch try registry.resolve() {
        case .setup:
            return .setup

        case let .picker(slots):
            return .picker(try slots.map { try IdentityCandidate(native: $0) })

        case let .ready(slot, key):
            return .ready(try IdentityCandidate(native: slot), key.peerId())

        case let .recovery(id, reason, peerId):
            return .recovery(
                try id.map { try IdentityLocation(native: $0) },
                IdentityRecoveryReason(native: reason), peerId: peerId)
        }
    }

    func open(_ location: IdentityLocation) throws -> IdentityKey {
        guard case let .ready(_, key) = try registry.inspect(id: location.marker) else {
            throw IdentitySlotError.notReady
        }
        return key
    }

    func open(_ slot: IdentitySlot) throws -> IdentityKey { try open(.slot(slot.id)) }

    /// Retain actual admission through blocking completion; never return an escaping native worker.
    func withIdentity<Value>(
        _ location: IdentityLocation, peerId: String,
        _ work: (String, IdentityKey) throws -> Value
    ) throws -> Value {
        let operation = try registry.operation(id: location.marker, expectedPeer: peerId)
        return try withExtendedLifetime(operation) {
            try work(operation.slot().directory, operation.key())
        }
    }

    @discardableResult
    func select(_ location: IdentityLocation) throws -> IdentityUpdate {
        try IdentityUpdate(native: registry.select(id: location.marker))
    }

    /// Core create commits selection once; callers must not perform a second select.
    func create() throws -> (slot: IdentitySlot, peerId: String, update: IdentityUpdate) {
        let update = try IdentityUpdate(native: registry.create())
        guard case let .ready(candidate, peerId) = update.resolution,
            case let .slot(id) = candidate.location
        else { throw IdentitySlotError.notReady }
        return (IdentitySlot(id: id, state: candidate.state), peerId, update)
    }

    @discardableResult
    func remove(_ id: UUID, confirming peerID: String) throws -> String? {
        try registry.remove(id: id.uuidString.lowercased(), confirmedPeer: peerID)
    }
}
