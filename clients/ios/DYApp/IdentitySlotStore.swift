import Darwin
import Foundation

/// On-disk UUID slots shared by the iOS and macOS shells. Startup selection is handled separately.
struct IdentitySlot {
    enum State: String {
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
}

/// Slot creation is serialized across scenes, windows and app processes by a root advisory lock.
/// An interrupted create remains visible as pending or damaged and never regenerates its key.
final class IdentitySlotStore {
    private let root: URL
    private let files = FileManager.default
    private let newID: () -> UUID
    private var profiles: URL { root.appendingPathComponent("profiles", isDirectory: true) }

    init(root: URL, newID: @escaping () -> UUID = { UUID() }) {
        self.root = root
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
