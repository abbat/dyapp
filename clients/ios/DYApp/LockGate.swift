import SwiftUI

/// The iOS app lock around the start screen: until the saved choice allows it, only the setup or
/// unlock screen exists, so no profile, message or Peer ID renders while the app is locked.
struct LockGate: View {
    @StateObject private var lock = LockModel()

    var body: some View {
        switch lock.screen {
        case .setup:
            SetupView(lock: lock)

        case .unlock:
            UnlockView(lock: lock)

        case .open:
            ContentView()

        case .failed:
            Text("app.locked").accessibilityIdentifier("app-locked")
        }
    }
}

/// Lock state for the gate. PIN work runs off the main thread: the Argon2id KDF takes 64 MiB.
@MainActor
final class LockModel: ObservableObject {
    /// What the gate shows.
    enum Screen { case setup, unlock, open, failed }

    /// Shortest accepted PIN; the Rust verifier enforces 6 to 64 digits.
    static let minPin = 6

    @Published private(set) var screen = Screen.failed
    /// Catalog key of the message under the PIN field.
    @Published private(set) var notice: String?
    @Published private(set) var busy = false
    private let store: AppLockStore?
    private var unlocked = false

    init() {
        store = try? AppLockStore.onDevice(directory: RustBridge.directory())
        #if DEBUG
        // UI tests start from a known lock state; release builds have no such argument.
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("--app-lock-new") || arguments.contains("--app-lock-off") {
            try? KeychainAppLockVault().delete()
        }
        if arguments.contains("--app-lock-off") { try? store?.skip() }
        #endif
        refresh()
    }

    /// First-launch Skip: the start screen opens and no PIN is set.
    func skip() {
        run { store in
            try store.skip()
            return (false, nil)
        }
    }

    /// First-launch Set PIN after both entries match.
    func enroll(_ pin: String, repeated: String) {
        guard pin.count >= Self.minPin else { notice = "lock.pin.short"; return }
        guard pin == repeated else { notice = "lock.pin.mismatch"; return }
        run { store in
            try store.enroll(pin)
            return (true, nil)
        }
    }

    /// One counted PIN attempt; five failures leave only the device-credential notice.
    func unlock(_ pin: String) {
        run { store in
            switch try store.attempt(pin) {
            case .unlocked:
                (true, nil)

            case .wrong:
                (false, "lock.pin.wrong")

            case .deviceCredentialRequired:
                (false, "lock.device.credential")
            }
        }
    }

    private func refresh() {
        guard let mode = try? store?.mode() else { screen = .failed; return }
        switch mode {
        case .new:
            screen = .setup

        case .off:
            screen = .open

        case .pin:
            screen = unlocked ? .open : .unlock

        case .biometric:
            // Biometric unlock is planned; until then the app stays locked.
            screen = .failed
        }
    }

    private func run(_ action: @escaping @Sendable (AppLockStore) throws -> (unlocked: Bool, notice: String?)) {
        guard let store, !busy else { return }
        busy = true
        notice = nil
        Task {
            let result = await Task.detached { Result { try action(store) } }.value
            busy = false
            switch result {
            case let .success(outcome):
                unlocked = unlocked || outcome.unlocked
                notice = outcome.notice

            case .failure:
                notice = "lock.failed"
            }
            refresh()
        }
    }
}

/// A digits-only PIN entry of at most 64 digits.
private struct PinField: View {
    let title: LocalizedStringKey
    let tag: String
    @Binding var pin: String

    var body: some View {
        SecureField(title, text: $pin)
            .keyboardType(.numberPad)
            .textFieldStyle(.roundedBorder)
            .accessibilityIdentifier(tag)
            .onChange(of: pin) { value in
                let digits = String(value.filter { $0.isASCII && $0.isNumber }.prefix(64))
                if digits != value { pin = digits }
            }
    }
}

/// The message under the PIN fields, if any.
private struct Notice: View {
    let key: String?

    var body: some View {
        if let key {
            Text(LocalizedStringKey(key)).foregroundColor(.red).accessibilityIdentifier("lock-notice")
        }
    }
}

/// First launch: Skip or Set PIN.
private struct SetupView: View {
    @ObservedObject var lock: LockModel
    @State private var pin = ""
    @State private var repeated = ""

    var body: some View {
        VStack(spacing: 16) {
            Text("lock.setup.title").font(.title)
            Text("lock.setup.body")
            PinField(title: "lock.pin", tag: "lock-pin", pin: $pin)
            PinField(title: "lock.pin.repeat", tag: "lock-pin-repeat", pin: $repeated)
            Notice(key: lock.notice)
            Button("lock.set") { lock.enroll(pin, repeated: repeated) }
                .accessibilityIdentifier("lock-set")
            Button("lock.skip") { lock.skip() }
                .accessibilityIdentifier("lock-skip")
        }
        .disabled(lock.busy)
        .padding()
    }
}

/// The only screen of a PIN-locked app.
private struct UnlockView: View {
    @ObservedObject var lock: LockModel
    @State private var pin = ""

    var body: some View {
        VStack(spacing: 16) {
            Text("app.locked").font(.title).accessibilityIdentifier("app-locked")
            PinField(title: "lock.pin", tag: "lock-pin", pin: $pin)
            Notice(key: lock.notice)
            Button("lock.unlock") {
                lock.unlock(pin)
                pin = ""
            }
            .accessibilityIdentifier("lock-unlock")
            .disabled(pin.count < LockModel.minPin)
        }
        .disabled(lock.busy)
        .padding()
    }
}
