import Foundation

/// Protected credentials only; the common registry owns paths, lifecycle and selection policy.
final class NativeSlotKeys: MobileSlotKeys {
    private let legacy: PlatformKeyStore
    private let eraseKey: @Sendable (UUID) throws -> Void

    init(
        legacy: PlatformKeyStore = .shared,
        eraseKey: @escaping @Sendable (UUID) throws -> Void = {
            try PlatformKeyStore.eraseWrappingKey(forSlot: $0)
        }
    ) {
        self.legacy = legacy
        self.eraseKey = eraseKey
    }

    private static func slot(_ id: String) throws -> UUID {
        guard let uuid = UUID(uuidString: id), uuid.uuidString.lowercased() == id else {
            throw WrapError.Failed(reason: "Invalid slot credential identifier")
        }
        return uuid
    }

    private func store(_ id: String) throws -> PlatformKeyStore {
        id == "legacy-primary" ? legacy : .forSlot(try Self.slot(id))
    }

    func contains(slotId: String) throws -> Bool {
        try store(slotId).containsWrappingKey()
    }

    func openWrapped(slotId: String, wrapped: Data) throws -> IdentityKey? {
        try credentialCall {
            let store = try self.store(slotId)
            guard try store.containsWrappingKey() else { return nil }
            return try IdentityKey.unwrap(store: store, wrapped: wrapped)
        }
    }

    func wrap(slotId: String, key: IdentityKey) throws -> Data {
        try credentialCall { try key.wrap(store: store(slotId).requiringFreshKey()) }
    }

    func wrapExisting(slotId: String, key: IdentityKey) throws -> Data {
        try credentialCall { try key.wrap(store: store(slotId).requiringExistingKey()) }
    }

    func erase(slotId: String) throws {
        // Core refuses legacy removal; the platform eraser cannot address that account either.
        try credentialCall { try eraseKey(Self.slot(slotId)) }
    }

    private func credentialCall<T>(_ action: () throws -> T) throws -> T {
        do {
            return try action()
        } catch let error as WrapError {
            throw error
        } catch {
            throw WrapError.Failed(reason: "Native credential operation failed")
        }
    }
}
