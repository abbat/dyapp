import Foundation

struct AppLockLoad {
    let store: AppLockStore
    let mode: Result<StoredLock, Error>
}

struct AppLockChange {
    let outcome: Result<(unlocked: Bool, notice: String?), Error>
    let mode: Result<StoredLock, Error>
}

// Transfers one owned native result; consumers run only on the main actor.
private struct AppLockIOResult<T>: @unchecked Sendable {
    let value: Result<T, Error>
}

/// Synchronous Keychain/KDF work has a dedicated queue, not the UI or cooperative executor.
enum AppLockIO {
    private static let queue = DispatchQueue(label: "com.dyapp.app-lock", qos: .userInitiated)

    static func makeStore() throws -> AppLockStore {
        let store = try AppLockStore.onDevice(directory: RustBridge.directory())
        #if DEBUG
            let arguments = ProcessInfo.processInfo.arguments
            if arguments.contains("--app-lock-new") || arguments.contains("--app-lock-off") {
                try KeychainAppLockVault().delete()
            }
            if arguments.contains("--app-lock-off") { try store.skip() }
        #endif
        return store
    }

    static func perform<T>(
        _ work: @escaping @Sendable () throws -> T,
        completion: @escaping @MainActor @Sendable (Result<T, Error>) -> Void
    ) {
        queue.async {
            let output = AppLockIOResult(value: Result { try work() })
            Task { @MainActor in completion(output.value) }
        }
    }
}
