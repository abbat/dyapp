import Foundation
import Security

/// One protected item holding the whole app-lock record.
protocol AppLockVault {
    func load() throws -> Data?
    func save(_ data: Data) throws
    func delete() throws
}

typealias PinAttempt = MobilePinAttempt
typealias StoredLock = MobileAppLockMode

/// Platform ownership facade; the shared core owns record layout, counters and transitions.
/// All calls block and belong off the UI thread. Recovery requires fresh OS authentication.
final class AppLockStore: @unchecked Sendable {
    private static let ioLock = NSLock()
    private let core: MobileAppLock

    init(vault: AppLockVault) {
        core = MobileAppLock(format: .ios, storage: AppLockProtectedStorage(vault: vault))
    }

    /// Keychain items survive uninstall; a new installation discards only its old app-lock item.
    static func onDevice(directory: URL) throws -> AppLockStore {
        let vault = KeychainAppLockVault()
        let marker = directory.appendingPathComponent("app-lock-installed")
        if !FileManager.default.fileExists(atPath: marker.path) {
            try vault.delete()
            try Data().write(to: marker, options: .atomic)
        }
        return AppLockStore(vault: vault)
    }

    func mode() throws -> StoredLock { try withCore { try $0.mode() } }

    /// Protected replacement requires a fresh successful platform device-credential check.
    func useBiometric(afterDeviceCredential: Bool = false) throws {
        try withCore {
            if afterDeviceCredential {
                try $0.useBiometric()
            } else {
                try $0.useBiometricInitial()
            }
        }
    }

    func skip() throws { try withCore { try $0.skip() } }

    func enroll(_ pinValue: String) throws {
        try withCore { try $0.enrollInitial(pin: pinValue) }
    }

    func attempt(_ pinValue: String) throws -> PinAttempt {
        try withCore { try $0.attempt(pin: pinValue) }
    }

    func change(current: String, newPin: String?) throws -> PinAttempt {
        try withCore { try $0.change(current: current, newPin: newPin) }
    }

    /// Invoke only after a fresh OS device-credential or biometric check.
    func resetAfterDeviceCredential(_ newPin: String?) throws {
        try withCore { try $0.resetAfterDeviceCredential(newPin: newPin) }
    }

    private func withCore<T>(_ action: (MobileAppLock) throws -> T) rethrows -> T {
        Self.ioLock.lock()
        defer { Self.ioLock.unlock() }
        return try action(core)
    }
}

/// Protected IO only; all callbacks are serialized by their owning facade/core handle.
private final class AppLockProtectedStorage: MobileAppLockStorage, @unchecked Sendable {
    private let vault: AppLockVault

    init(vault: AppLockVault) { self.vault = vault }

    private static func failed() -> AppLockStorageError {
        // Never pass arbitrary backend error descriptions containing credentials across FFI.
        .Failed(reason: "App-lock protected storage unavailable")
    }

    func readEntry(entry: MobileAppLockEntry) throws -> Data? {
        try requireRecord(entry)
        do { return try vault.load() } catch { throw Self.failed() }
    }

    func writeEntry(entry: MobileAppLockEntry, data: Data) throws {
        try requireRecord(entry)
        do { try vault.save(data) } catch { throw Self.failed() }
    }

    func removeEntry(entry: MobileAppLockEntry) throws {
        try requireRecord(entry)
        do { try vault.delete() } catch { throw Self.failed() }
    }

    private func requireRecord(_ entry: MobileAppLockEntry) throws {
        guard entry == .record else {
            throw AppLockStorageError.Failed(reason: "Unsupported iOS app-lock storage entry")
        }
    }
}

/// The app-lock item: readable only on this device after its first unlock, never backed up.
struct KeychainAppLockVault: AppLockVault {
    private var query: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "com.dyapp.app-lock",
            kSecAttrAccount as String: "app-lock",
        ]
    }

    private static func failed(_ status: OSStatus) -> Error {
        NSError(domain: NSOSStatusErrorDomain, code: Int(status))
    }

    func load() throws -> Data? {
        var query = query
        query[kSecReturnData as String] = true
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = item as? Data else { throw Self.failed(status) }
        return data
    }

    // One item updated in place, so a crash leaves either the old or the new record.
    func save(_ data: Data) throws {
        let status = SecItemUpdate(
            query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecSuccess { return }
        guard status == errSecItemNotFound else { throw Self.failed(status) }
        var item = query
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let added = SecItemAdd(item as CFDictionary, nil)
        guard added == errSecSuccess else { throw Self.failed(added) }
    }

    func delete() throws {
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw Self.failed(status)
        }
    }
}
