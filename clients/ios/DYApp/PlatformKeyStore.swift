import CryptoKit
import Foundation
import Security

/// The core's `KeyStore` over a random AES-256-GCM key kept in the Keychain, readable only on
/// this device after its first unlock. A wrapped blob is CryptoKit's combined nonce, ciphertext
/// and tag.
final class PlatformKeyStore: KeyStore {
    /// The original root identity keeps its existing Keychain account on upgrade.
    static let shared = PlatformKeyStore(account: "identity-wrap")

    private let account: String

    private var query: [String: Any] { [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.dyapp.identity-wrap",
        kSecAttrAccount as String: account,
    ] }

    private init(account: String) { self.account = account }

    static func forSlot(_ slot: UUID) -> PlatformKeyStore {
        PlatformKeyStore(account: "profile:\(slot.uuidString.lowercased())")
    }

    /// Erase and verify only one UUID slot's wrapping item; the legacy account is not addressable here.
    static func eraseWrappingKey(forSlot slot: UUID) throws {
        let store = forSlot(slot)
        let status = SecItemDelete(store.query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw WrapError.Failed(reason: "Keychain delete: OSStatus \(status)")
        }
        guard try !store.containsWrappingKey() else {
            throw WrapError.Failed(reason: "wrapping key remains after delete")
        }
    }

    /// Check an account before reserving a new slot; never reuse a stranded wrapping key.
    func containsWrappingKey() throws -> Bool { try storedKey() != nil }

    private func storedKey() throws -> SymmetricKey? {
        var query = self.query
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else {
            throw WrapError.Failed(reason: "Keychain read: OSStatus \(status)")
        }
        return SymmetricKey(data: data)
    }

    private func createKey() throws -> SymmetricKey {
        let key = SymmetricKey(size: .bits256)
        var item = query
        item[kSecValueData as String] = key.withUnsafeBytes { Data($0) }
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw WrapError.Failed(reason: "Keychain write: OSStatus \(status)")
        }
        return key
    }

    func wrap(secret: Data) throws -> Data {
        try keystoreCall {
            let key = try storedKey() ?? createKey()
            guard let blob = try AES.GCM.seal(secret, using: key).combined else {
                throw WrapError.Failed(reason: "no combined GCM output")
            }
            return blob
        }
    }

    func unwrap(wrapped: Data) throws -> Data {
        try keystoreCall {
            // A missing key is an error, not a reason to make a new one: the blob stays unreadable.
            guard let key = try storedKey() else {
                throw WrapError.Failed(reason: "the wrapping key is gone")
            }
            return try AES.GCM.open(AES.GCM.SealedBox(combined: wrapped), using: key)
        }
    }

    // The core expects `WrapError`; the reason names the failure, never key bytes.
    private func keystoreCall(_ block: () throws -> Data) throws -> Data {
        do {
            return try block()
        } catch let error as WrapError {
            throw error
        } catch {
            throw WrapError.Failed(reason: "\(type(of: error)): \(error)")
        }
    }
}
