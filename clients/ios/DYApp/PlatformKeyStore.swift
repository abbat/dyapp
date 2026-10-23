import CryptoKit
import Foundation
import Security

/// The core's `KeyStore` over a random AES-256-GCM key kept in the Keychain, readable only on
/// this device after its first unlock. A wrapped blob is CryptoKit's combined nonce, ciphertext
/// and tag.
final class PlatformKeyStore: KeyStore {
    static let shared = PlatformKeyStore()

    // Computed: a stored [String: Any] is not Sendable, so Swift 6 refuses it as a static.
    private static var query: [String: Any] { [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.dyapp.identity-wrap",
        kSecAttrAccount as String: "identity-wrap",
    ] }

    private static func storedKey() throws -> SymmetricKey? {
        var query = Self.query
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else {
            throw WrapError.Failed(reason: "Keychain read: OSStatus \(status)")
        }
        return SymmetricKey(data: data)
    }

    private static func createKey() throws -> SymmetricKey {
        let key = SymmetricKey(size: .bits256)
        var item = Self.query
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
            let key = try Self.storedKey() ?? Self.createKey()
            guard let blob = try AES.GCM.seal(secret, using: key).combined else {
                throw WrapError.Failed(reason: "no combined GCM output")
            }
            return blob
        }
    }

    func unwrap(wrapped: Data) throws -> Data {
        try keystoreCall {
            // A missing key is an error, not a reason to make a new one: the blob stays unreadable.
            guard let key = try Self.storedKey() else {
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
