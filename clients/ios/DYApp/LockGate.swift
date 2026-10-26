import SwiftUI
import UIKit.UIGestureRecognizerSubclass

/// The iOS app lock around the start screen: until the saved choice allows it, only the setup or
/// unlock screen exists, so no profile, message or Peer ID renders while the app is locked.
struct LockGate: View {
    @Environment(\.scenePhase) private var phase
    @StateObject private var lock = LockModel()

    var body: some View {
        screen
            .overlay {
                // Covered from `inactive` on, so the app-switcher snapshot shows no content.
                if lock.guarded && phase != .active {
                    Color(.systemBackground).ignoresSafeArea()
                }
            }
            .onAppear { lock.phase(phase) }
            .onChange(of: phase) { lock.phase($0) }
    }

    @ViewBuilder private var screen: some View {
        switch lock.screen {
        case .setup:
            SetupView(lock: lock)

        case .unlock:
            UnlockView(lock: lock)

        case .open:
            if lock.settingsOpen {
                SettingsView(lock: lock)
            } else {
                VStack {
                    ContentView()
                    Button("lock.settings") { lock.openSettings(true) }
                        .accessibilityIdentifier("lock-settings")
                        .padding()
                }
            }

        case .failed:
            Text("app.locked").accessibilityIdentifier("app-locked")
        }
    }
}

/// The unlocked-session deadline: 60 s after backgrounding, or five minutes after the last
/// foreground touch. Times are milliseconds on a monotonic clock; once expired it stays expired.
struct AppLockTimer {
    static let backgroundGrace: UInt64 = 60_000
    static let foregroundIdle: UInt64 = 300_000

    private(set) var deadline: UInt64
    private var inBackground = false

    init(now: UInt64) {
        deadline = now + Self.foregroundIdle
    }

    /// Counts device sleep too, unlike `systemUptime`.
    static func uptime() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000
    }

    func expired(at now: UInt64) -> Bool { now >= deadline }

    /// Only a foreground touch on a live session restarts the idle limit.
    mutating func interaction(at now: UInt64) {
        if !inBackground && !expired(at: now) { deadline = now + Self.foregroundIdle }
    }

    mutating func background(at now: UInt64) {
        if !expired(at: now) { deadline = now + Self.backgroundGrace }
        inBackground = true
    }

    mutating func foreground(at now: UInt64) {
        if !expired(at: now) { deadline = now + Self.foregroundIdle }
        inBackground = false
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
    @Published private(set) var mode = StoredLock.new
    /// Whether the open app shows its lock settings in place of the start screen.
    @Published private(set) var settingsOpen = false
    /// Catalog key of the message under the PIN field.
    @Published private(set) var notice: String?
    @Published private(set) var busy = false
    private let store: AppLockStore?
    /// The unlocked session; nil until a PIN entry unlocks the app.
    private var session: AppLockTimer?
    private var expiry: Task<Void, Never>?

    /// Whether a lock is set, so the app is covered whenever it is not active.
    var guarded: Bool { mode == .pin || mode == .biometric }

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

    /// The gate's reading of a counted PIN attempt; five failures leave only the
    /// device-credential notice.
    nonisolated private static func outcome(_ attempt: PinAttempt) -> (unlocked: Bool, notice: String?) {
        switch attempt {
        case .unlocked:
            (true, nil)

        case .wrong:
            (false, "lock.pin.wrong")

        case .deviceCredentialRequired:
            (false, "lock.device.credential")
        }
    }

    /// Catalog key of the reason a new PIN is refused, if any.
    private static func rejection(_ pin: String, _ repeated: String) -> String? {
        if pin.count < Self.minPin { return "lock.pin.short" }
        return pin == repeated ? nil : "lock.pin.mismatch"
    }

    /// First-launch Skip: the start screen opens and no PIN is set.
    func skip() {
        run { store in
            try store.skip()
            return (false, nil)
        }
    }

    /// Set PIN, on first launch or in settings while none is set, after both entries match.
    func enroll(_ pin: String, repeated: String) {
        if let rejection = Self.rejection(pin, repeated) { notice = rejection; return }
        run { store in
            try store.enroll(pin)
            return (true, nil)
        }
    }

    /// One counted PIN attempt from the unlock screen.
    func unlock(_ pin: String) {
        run { store in Self.outcome(try store.attempt(pin)) }
    }

    /// Settings: replace the PIN, or turn it off when `newPin` is nil, only if `current` verifies;
    /// the check counts as an unlock attempt.
    func change(current: String, newPin: String?, repeated: String) {
        if let newPin, let rejection = Self.rejection(newPin, repeated) { notice = rejection; return }
        run { store in Self.outcome(try store.change(current: current, newPin: newPin)) }
    }

    func openSettings(_ open: Bool) {
        notice = nil
        settingsOpen = open
    }

    /// The background grace starts on `background`; every phase change applies an expired deadline,
    /// so a late return locks while the cover still hides the content.
    func phase(_ phase: ScenePhase) {
        if phase == .active {
            watchTouches()
            session?.foreground(at: AppLockTimer.uptime())
        } else if phase == .background {
            session?.background(at: AppLockTimer.uptime())
        }
        expireIfDue()
    }

    // ponytail: touches on the app's windows only; keyboard typing lives in a separate system
    // window, so count text edits too once a screen takes long text input.
    private func watchTouches() {
        let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap(\.windows)
        for window in windows where !(window.gestureRecognizers ?? []).contains(where: { $0 is TouchWatcher }) {
            window.addGestureRecognizer(TouchWatcher { [weak self] in
                self?.session?.interaction(at: AppLockTimer.uptime())
                self?.expireIfDue()
            })
        }
    }

    /// Sleeps until the deadline, which touches may have moved, and locks once it passes.
    private func watchExpiry() {
        expiry?.cancel()
        expiry = Task { [weak self] in
            while let deadline = self?.session?.deadline, !Task.isCancelled {
                let now = AppLockTimer.uptime()
                if now < deadline { try? await Task.sleep(nanoseconds: (deadline - now) * 1_000_000) }
                self?.expireIfDue()
            }
        }
    }

    private func expireIfDue() {
        guard let session, session.expired(at: AppLockTimer.uptime()) else { return }
        self.session = nil
        notice = nil
        settingsOpen = false
        refresh()
    }

    private func refresh() {
        guard let mode = try? store?.mode() else { screen = .failed; return }
        self.mode = mode
        switch mode {
        case .new:
            screen = .setup

        case .off:
            screen = .open

        case .pin:
            screen = session == nil ? .unlock : .open

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
                if outcome.unlocked {
                    session = AppLockTimer(now: AppLockTimer.uptime())
                    watchExpiry()
                }
                notice = outcome.notice

            case .failure:
                notice = "lock.failed"
            }
            refresh()
        }
    }
}

/// Reports each touch on its window and then fails, so it never delays or cancels other gestures.
private final class TouchWatcher: UIGestureRecognizer {
    private let onTouch: () -> Void

    init(onTouch: @escaping () -> Void) {
        self.onTouch = onTouch
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesEnded = false
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesBegan(touches, with: event)
        onTouch()
        state = .failed
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

/// App-lock settings in place of the start screen: Set PIN while none is set; with a PIN, Change
/// and Turn off both need the current PIN. Every submit clears the typed PINs.
private struct SettingsView: View {
    @ObservedObject var lock: LockModel
    @State private var current = ""
    @State private var pin = ""
    @State private var repeated = ""

    var body: some View {
        let pinSet = lock.mode == .pin
        let verified = !pinSet || current.count >= LockModel.minPin
        VStack(spacing: 16) {
            Text(LocalizedStringKey(pinSet ? "lock.mode.pin" : "lock.mode.off")).accessibilityIdentifier("lock-mode")
            if pinSet {
                PinField(title: "lock.pin.current", tag: "lock-pin-current", pin: $current)
            }
            PinField(title: "lock.pin.new", tag: "lock-pin", pin: $pin)
            PinField(title: "lock.pin.repeat", tag: "lock-pin-repeat", pin: $repeated)
            Notice(key: lock.notice)
            Button(LocalizedStringKey(pinSet ? "lock.change" : "lock.set")) { submit(pin) }
                .accessibilityIdentifier("lock-set")
                .disabled(!verified)
            if pinSet {
                Button("lock.turn.off") { submit(nil) }
                    .accessibilityIdentifier("lock-off")
                    .disabled(!verified)
            }
            Button("lock.back") { lock.openSettings(false) }
                .accessibilityIdentifier("lock-back")
        }
        .disabled(lock.busy)
        .padding()
    }

    private func submit(_ newPin: String?) {
        if lock.mode == .pin {
            lock.change(current: current, newPin: newPin, repeated: repeated)
        } else {
            lock.enroll(pin, repeated: repeated)
        }
        current = ""
        pin = ""
        repeated = ""
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
