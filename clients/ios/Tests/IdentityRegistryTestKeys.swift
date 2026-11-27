@testable import DYApp
import Foundation

/// The real core chooses IDs; its synchronous contains callback admits a controlled collision.
final class IdentityCollisionKeys: MobileSlotKeys, @unchecked Sendable {
    enum Kind { case credential, directory }

    private let root: URL
    private let kind: Kind
    private let native = NativeSlotKeys()
    private let lock = NSLock()
    private var first: String?
    private var blob: Data?
    private var peer: String?

    var collision: (id: String?, blob: Data?, peer: String?) {
        lock.lock()
        defer { lock.unlock() }
        return (first, blob, peer)
    }

    init(root: URL, kind: Kind) {
        self.root = root
        self.kind = kind
    }

    func contains(slotId: String) throws -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard slotId != "legacy-primary", first == nil else {
            return try native.contains(slotId: slotId)
        }
        if try native.contains(slotId: slotId) { return true }
        let folder = root.appendingPathComponent("profiles").appendingPathComponent(slotId)
        if FileManager.default.fileExists(atPath: folder.path) { return false }
        first = slotId
        switch kind {
        case .credential:
            let key = try IdentityKey.generate()
            blob = try native.wrap(slotId: slotId, key: key)
            peer = key.peerId()
            return true

        case .directory:
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
            try Data("pending\n".utf8).write(to: folder.appendingPathComponent("state"))
            try Data("preserve collision".utf8).write(to: folder.appendingPathComponent("sentinel"))
            return false
        }
    }

    func openWrapped(slotId: String, wrapped: Data) throws -> IdentityKey? {
        try native.openWrapped(slotId: slotId, wrapped: wrapped)
    }

    func wrap(slotId: String, key: IdentityKey) throws -> Data {
        try native.wrap(slotId: slotId, key: key)
    }

    func wrapExisting(slotId: String, key: IdentityKey) throws -> Data {
        try native.wrapExisting(slotId: slotId, key: key)
    }

    func erase(slotId: String) throws { try native.erase(slotId: slotId) }
}

final class IdentityEraseControl: @unchecked Sendable {
    private struct Refused: Error {}
    private let lock = NSLock()
    private var refused = true

    func permit() {
        lock.lock()
        refused = false
        lock.unlock()
    }

    func erase(_ id: UUID) throws {
        lock.lock()
        let shouldRefuse = refused
        lock.unlock()
        if shouldRefuse { throw Refused() }
        try PlatformKeyStore.eraseWrappingKey(forSlot: id)
    }
}
