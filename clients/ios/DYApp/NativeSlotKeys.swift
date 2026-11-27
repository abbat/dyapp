import Foundation

/// Protected credentials only; the common registry owns paths, lifecycle and selection policy.
final class NativeSlotKeys: MobileSlotKeys {
    private static func slot(_ id: String) throws -> UUID {
        guard let uuid = UUID(uuidString: id), uuid.uuidString.lowercased() == id else {
            throw WrapError.Failed(reason: "Invalid slot credential identifier")
        }
        return uuid
    }

    private static func store(_ id: String) throws -> PlatformKeyStore {
        id == "legacy-primary" ? .shared : .forSlot(try slot(id))
    }

    func contains(slotId: String) throws -> Bool {
        try Self.store(slotId).containsWrappingKey()
    }

    func openWrapped(slotId: String, wrapped: Data) throws -> IdentityKey? {
        try credentialCall {
            let store = try Self.store(slotId)
            guard try store.containsWrappingKey() else { return nil }
            return try IdentityKey.unwrap(store: store, wrapped: wrapped)
        }
    }

    func wrap(slotId: String, key: IdentityKey) throws -> Data {
        try credentialCall { try key.wrap(store: Self.store(slotId).requiringFreshKey()) }
    }

    func wrapExisting(slotId: String, key: IdentityKey) throws -> Data {
        try credentialCall { try key.wrap(store: Self.store(slotId).requiringExistingKey()) }
    }

    func erase(slotId: String) throws {
        // Core refuses legacy removal; the platform eraser cannot address that account either.
        try PlatformKeyStore.eraseWrappingKey(forSlot: Self.slot(slotId))
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
