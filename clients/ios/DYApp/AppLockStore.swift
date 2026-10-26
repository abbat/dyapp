import Foundation
import Security

/// PIN work comes from the shared Rust FFI; unit tests use a deterministic fake.
struct AppPinVerifier {
    // Computed: a stored struct of closures is not Sendable, so Swift 6 refuses it as a static.
    static var rust: Self {
        Self(hash: { try hashAppPin(pin: $0) }, verify: { try verifyAppPin(pin: $0, record: $1) })
    }

    let hash: (String) throws -> Data
    let verify: (String, Data) throws -> Bool
}

/// One protected item holding the whole app-lock record.
protocol AppLockVault {
    func load() throws -> Data?
    func save(_ data: Data) throws
    func delete() throws
}

enum PinAttempt { case unlocked, wrong, deviceCredentialRequired }

/// The saved app-lock choice: either a PIN or device biometrics, never both.
enum StoredLock: UInt8 {
    /// First launch: neither Skip nor a lock has been chosen. Never stored.
    case new = 255
    /// Skipped or turned off.
    case off = 0
    /// The app unlocks with its PIN.
    case pin = 1
    /// The app unlocks with device biometrics or the device passcode.
    case biometric = 2
}

enum AppLockError: Error { case invalidRecord, notEnrolled, alreadyEnrolled, notEnabled }

/// App-wide PIN verifier, failure counter and lock choice in one `ThisDeviceOnly` Keychain item,
/// apart from the identity Keychain items. The UI must authenticate with the OS before calling
/// `resetAfterDeviceCredential`. Every attempted PIN is persisted before KDF work, so a process
/// crash cannot erase a failed attempt. Layout: version, mode, failures, then the 49-byte Rust
/// verifier for a PIN only.
final class AppLockStore {
    private static let version: UInt8 = 1
    private static let recordSize = 49
    private static let maxFailures: UInt8 = 5

    private let vault: AppLockVault
    private let pin: AppPinVerifier

    init(vault: AppLockVault, pin: AppPinVerifier) {
        self.vault = vault
        self.pin = pin
    }

    /// Keychain items outlive an uninstall: a fresh install drops the item a previous one left,
    /// recognised by a marker in the app's own `directory`, which an uninstall deletes.
    static func onDevice(directory: URL) throws -> AppLockStore {
        let vault = KeychainAppLockVault()
        let marker = directory.appendingPathComponent("app-lock-installed")
        if !FileManager.default.fileExists(atPath: marker.path) {
            try vault.delete()
            try Data().write(to: marker, options: .atomic)
        }
        return AppLockStore(vault: vault, pin: .rust)
    }

    /// The saved choice; a corrupt record throws and must keep the app locked.
    func mode() throws -> StoredLock { try read()?.mode ?? .new }

    /// Replace any PIN with the biometric mode. Call on first launch, with the lock off, or
    /// after a fresh OS device-credential check.
    func useBiometric() throws { try write(.biometric) }

    /// Record the first-launch Skip choice; it leaves an existing lock in place.
    func skip() throws {
        if try read() == nil { try write(.off) }
    }

    /// Enrol on a fresh install only. A replacement requires `resetAfterDeviceCredential`.
    func enroll(_ pinValue: String) throws {
        guard try read()?.mode != .pin else { throw AppLockError.alreadyEnrolled }
        try write(.pin, record: pin.hash(pinValue))
    }

    /// Consume and persist one attempt before verifying. Five failures require OS authentication.
    func attempt(_ pinValue: String) throws -> PinAttempt {
        guard let state = try read(), state.mode == .pin else { throw AppLockError.notEnrolled }
        if state.failures >= Self.maxFailures { return .deviceCredentialRequired }
        let failures = state.failures + 1
        try write(.pin, failures: failures, record: state.record)
        if try pin.verify(pinValue, state.record) {
            try write(.pin, record: state.record)
            return .unlocked
        }
        return failures == Self.maxFailures ? .deviceCredentialRequired : .wrong
    }

    /// Settings: replace the PIN, or clear it when `newPin` is nil, only if `current` verifies.
    func change(current: String, newPin: String?) throws -> PinAttempt {
        let result = try attempt(current)
        if result == .unlocked { try resetAfterDeviceCredential(newPin) }
        return result
    }

    /// Call only after a fresh OS device-credential or biometric check; it replaces the PIN or
    /// the biometric mode with `newPin`, or turns the lock off when it is nil.
    func resetAfterDeviceCredential(_ newPin: String?) throws {
        guard let mode = try read()?.mode, mode != .off else { throw AppLockError.notEnabled }
        if let newPin {
            try write(.pin, record: pin.hash(newPin))
        } else {
            try write(.off)
        }
    }

    private func read() throws -> (mode: StoredLock, failures: UInt8, record: Data)? {
        guard let data = try vault.load() else { return nil }
        let bytes = [UInt8](data)
        guard bytes.count >= 3, bytes[0] == Self.version, let mode = StoredLock(rawValue: bytes[1]),
              mode != .new, bytes[2] <= Self.maxFailures,
              bytes.count == (mode == .pin ? 3 + Self.recordSize : 3)
        else { throw AppLockError.invalidRecord }
        return (mode, bytes[2], Data(bytes[3...]))
    }

    private func write(_ mode: StoredLock, failures: UInt8 = 0, record: Data = Data()) throws {
        guard record.count == (mode == .pin ? Self.recordSize : 0) else { throw AppLockError.invalidRecord }
        try vault.save(Data([Self.version, mode.rawValue, failures]) + record)
    }
}

/// The app-lock item: readable only on this device after its first unlock, never backed up.
struct KeychainAppLockVault: AppLockVault {
    private var query: [String: Any] { [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: "com.dyapp.app-lock",
        kSecAttrAccount as String: "app-lock",
    ] }

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
        let status = SecItemUpdate(query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
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
        guard status == errSecSuccess || status == errSecItemNotFound else { throw Self.failed(status) }
    }
}
