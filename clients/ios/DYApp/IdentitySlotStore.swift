import Darwin
import Foundation

/// On-disk UUID slots shared by the iOS and macOS shells. Startup selection is handled separately.
struct IdentitySlot {
    enum State: String, Sendable {
        case pending, ready, deleting, damaged
    }

    let id: UUID
    let state: State
}

enum IdentitySlotError: Error {
    case noFreeSlot
    case notReady
    case missingKey
    case nonRegularKey
    case reopenMismatch
    case lockFailed(Int32)
    case filesystem(Int32)
}

/// A UUID slot or the existing root identity, which keeps its original file and Keychain item.
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
}

struct IdentityCandidate: Equatable, Sendable {
    let location: IdentityLocation
    let state: IdentitySlot.State
}

enum IdentityRecoveryReason: Equatable {
    case missing, damaged, unavailable, invalidPointer, incomplete, deleting
}

enum IdentityResolution: Equatable {
    case setup
    case picker([IdentityCandidate])
    case ready(IdentityCandidate, String)
    case recovery(IdentityLocation?, IdentityRecoveryReason)
}

private enum ActivePointer {
    case absent, unselected, invalid, selected(IdentityLocation)
}

/// Slot creation is serialized across scenes, windows and app processes by a root advisory lock.
/// An interrupted create remains visible as pending or damaged and never regenerates its key.
final class IdentitySlotStore {
    private let root: URL
    private let files = FileManager.default
    private let legacyStore: PlatformKeyStore
    private let newID: () -> UUID
    private var profiles: URL { root.appendingPathComponent("profiles", isDirectory: true) }

    init(root: URL, legacyStore: PlatformKeyStore = .shared,
         newID: @escaping () -> UUID = { UUID() }) {
        self.root = root
        self.legacyStore = legacyStore
        self.newID = newID
    }

    private func directory(for id: UUID) -> URL {
        profiles.appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
    }

    private func marker(for id: UUID) -> URL { directory(for: id).appendingPathComponent("state") }
    private func keyFile(for id: UUID) -> URL { directory(for: id).appendingPathComponent("identity.key") }

    private func withLock<T>(_ body: () throws -> T) throws -> T {
        try files.createDirectory(at: root, withIntermediateDirectories: true)
        let lock = root.appendingPathComponent(".identity-slots.lock")
        let fd = Darwin.open(lock.path, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
        guard fd >= 0 else { throw IdentitySlotError.lockFailed(errno) }
        defer { _ = Darwin.close(fd) }
        guard flock(fd, LOCK_EX) == 0 else { throw IdentitySlotError.lockFailed(errno) }
        defer { _ = flock(fd, LOCK_UN) }
        return try body()
    }

    private func isRegularFile(_ url: URL) throws -> Bool {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        return values.isRegularFile == true && values.isSymbolicLink != true
    }

    private func state(for id: UUID) -> IdentitySlot.State {
        let marker = marker(for: id)
        guard (try? isRegularFile(marker)) == true,
              let data = try? Data(contentsOf: marker),
              let text = String(data: data, encoding: .utf8),
              let state = IdentitySlot.State(rawValue: text.trimmingCharacters(in: .newlines)),
              data == Data("\(state.rawValue)\n".utf8) else {
            return .damaged
        }
        return state
    }

    private func slotsWithoutLock() throws -> [IdentitySlot] {
        guard files.fileExists(atPath: profiles.path) else { return [] }
        return try files.contentsOfDirectory(at: profiles, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            .compactMap { url -> IdentitySlot? in
                guard let id = UUID(uuidString: url.lastPathComponent),
                      id.uuidString.lowercased() == url.lastPathComponent else { return nil }
                let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                let state: IdentitySlot.State = values?.isDirectory == true && values?.isSymbolicLink != true
                    ? self.state(for: id) : .damaged
                return IdentitySlot(id: id, state: state)
            }
            .sorted { $0.id.uuidString < $1.id.uuidString }
    }

    func slots() throws -> [IdentitySlot] { try withLock { try slotsWithoutLock() } }

    private func existsWithoutFollowing(_ url: URL) throws -> Bool {
        var metadata = stat()
        if url.path.withCString({ Darwin.lstat($0, &metadata) }) == 0 { return true }
        if errno == ENOENT { return false }
        throw IdentitySlotError.filesystem(errno)
    }

    private func candidatesWithoutLock() throws -> [IdentityCandidate] {
        var candidates = try slotsWithoutLock().map {
            IdentityCandidate(location: .slot($0.id), state: $0.state)
        }
        let legacy = root.appendingPathComponent("identity.key")
        if try existsWithoutFollowing(legacy) {
            let state: IdentitySlot.State = (try? isRegularFile(legacy)) == true ? .ready : .damaged
            candidates.insert(IdentityCandidate(location: .legacy, state: state), at: 0)
        }
        return candidates
    }

    func candidates() throws -> [IdentityCandidate] {
        try withLock { try candidatesWithoutLock() }
    }

    private func activePointerWithoutLock() throws -> ActivePointer {
        let file = root.appendingPathComponent("active-profile")
        guard try existsWithoutFollowing(file) else { return .absent }
        guard (try? isRegularFile(file)) == true,
              let data = try? Data(contentsOf: file), data.count <= 64,
              let text = String(data: data, encoding: .utf8), text.hasSuffix("\n") else {
            return .invalid
        }
        let value = String(text.dropLast())
        if value == "none" { return .unselected }
        if value == "legacy-primary" { return .selected(.legacy) }
        guard let id = UUID(uuidString: value), value == id.uuidString.lowercased() else {
            return .invalid
        }
        return .selected(.slot(id))
    }

    private func openKeyWithoutLock(_ candidate: IdentityCandidate) throws -> IdentityKey {
        guard candidate.state == .ready else { throw IdentitySlotError.notReady }
        switch candidate.location {
        case .legacy:
            let file = root.appendingPathComponent("identity.key")
            guard try existsWithoutFollowing(file) else { throw IdentitySlotError.missingKey }
            guard try isRegularFile(file) else { throw IdentitySlotError.nonRegularKey }
            return try IdentityKey.unwrap(
                store: legacyStore, wrapped: Data(contentsOf: file)
            )

        case .slot(let id):
            return try openWithoutLock(IdentitySlot(id: id, state: candidate.state))
        }
    }

    func open(_ location: IdentityLocation) throws -> IdentityKey {
        try withLock {
            guard let candidate = try candidatesWithoutLock().first(where: { $0.location == location }) else {
                throw IdentitySlotError.missingKey
            }
            return try openKeyWithoutLock(candidate)
        }
    }

    private func opened(_ candidate: IdentityCandidate) -> IdentityResolution {
        switch candidate.state {
        case .pending:
            return .recovery(candidate.location, .incomplete)

        case .deleting:
            return .recovery(candidate.location, .deleting)

        case .damaged:
            return .recovery(candidate.location, .damaged)

        case .ready:
            do {
                return .ready(candidate, try openKeyWithoutLock(candidate).peerId())
            } catch IdentitySlotError.missingKey {
                return .recovery(candidate.location, .missing)
            } catch {
                return .recovery(candidate.location, .unavailable)
            }
        }
    }

    private func fallback(_ candidates: [IdentityCandidate],
                          missing: IdentityLocation? = nil) -> IdentityResolution {
        if candidates.count == 1 { return opened(candidates[0]) }
        if candidates.isEmpty { return .recovery(missing, .missing) }
        return .picker(candidates)
    }

    private func emptyResolutionWithoutLock() throws -> IdentityResolution {
        if try legacyStore.containsWrappingKey() { return .recovery(.legacy, .missing) }
        return .setup
    }

    /// Resolve an existing identity without writing a pointer or generating a replacement key.
    func resolve() throws -> IdentityResolution {
        try withLock {
            let candidates = try candidatesWithoutLock()
            switch try activePointerWithoutLock() {
            case .selected(let location):
                if let candidate = candidates.first(where: { $0.location == location }) {
                    return opened(candidate)
                }
                if location == .legacy { return .recovery(.legacy, .missing) }
                return fallback(candidates, missing: location)

            case .invalid:
                if candidates.isEmpty { return .recovery(nil, .invalidPointer) }
                return fallback(candidates)

            case .unselected:
                if candidates.isEmpty { return try emptyResolutionWithoutLock() }
                return .picker(candidates)

            case .absent:
                if candidates.isEmpty { return try emptyResolutionWithoutLock() }
                return fallback(candidates)
            }
        }
    }

    /// Select only a ready identity whose key opens; never create one during selection.
    @discardableResult
    func select(_ location: IdentityLocation) throws -> String {
        try withLock {
            guard let candidate = try candidatesWithoutLock().first(where: { $0.location == location }),
                  case .ready(_, let peerId) = opened(candidate) else {
                throw IdentitySlotError.notReady
            }
            let file = root.appendingPathComponent("active-profile")
            try Data("\(location.marker)\n".utf8).write(to: file, options: .atomic)
            return peerId
        }
    }

    private func openWithoutLock(_ slot: IdentitySlot) throws -> IdentityKey {
        guard slot.state == .ready else { throw IdentitySlotError.notReady }
        let file = keyFile(for: slot.id)
        guard files.fileExists(atPath: file.path) else { throw IdentitySlotError.missingKey }
        guard try isRegularFile(file) else { throw IdentitySlotError.nonRegularKey }
        return try IdentityKey.unwrap(
            store: PlatformKeyStore.forSlot(slot.id), wrapped: Data(contentsOf: file)
        )
    }

    func open(_ slot: IdentitySlot) throws -> IdentityKey {
        try withLock {
            guard let current = try slotsWithoutLock().first(where: { $0.id == slot.id }) else {
                throw IdentitySlotError.missingKey
            }
            return try openWithoutLock(current)
        }
    }

    /// Reserve a fresh UUID directory before creating its Keychain item. No collision is overwritten.
    func create() throws -> (slot: IdentitySlot, peerId: String) {
        try withLock {
            try files.createDirectory(at: profiles, withIntermediateDirectories: true)
            for _ in 0..<8 {
                let id = newID()
                let store = PlatformKeyStore.forSlot(id)
                if try store.containsWrappingKey() { continue }
                let path = directory(for: id)
                // mkdir is exclusive. A racing or stranded directory cannot be reused.
                if Darwin.mkdir(path.path, S_IRWXU) != 0 {
                    if errno == EEXIST { continue }
                    throw IdentitySlotError.lockFailed(errno)
                }
                let pending = IdentitySlot(id: id, state: .pending)
                try Data("pending\n".utf8).write(to: marker(for: id), options: .atomic)
                let key = IdentityKey.generate()
                let peerId = key.peerId()
                try key.wrap(store: store).write(to: keyFile(for: id), options: .atomic)
                let reopened = try IdentityKey.unwrap(
                    store: store, wrapped: Data(contentsOf: keyFile(for: id))
                )
                guard reopened.peerId() == peerId else { throw IdentitySlotError.reopenMismatch }
                try Data("ready\n".utf8).write(to: marker(for: id), options: .atomic)
                return (IdentitySlot(id: id, state: .ready), peerId)
            }
            throw IdentitySlotError.noFreeSlot
        }
    }
}
