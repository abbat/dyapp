import LocalAuthentication
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

    /// Whether device biometrics can be offered in place of the PIN.
    enum Biometrics { case ready, notEnrolled, absent }

    /// Shortest accepted PIN; the Rust verifier enforces 6 to 64 digits.
    static let minPin = 6

    #if DEBUG
    /// Simulators have no passcode: UI tests pass `--device-passcode` to stand in for one that
    /// always checks out.
    private static let fakePasscode = ProcessInfo.processInfo.arguments.contains("--device-passcode")
    /// Force the missing-passcode path even when a CI simulator has a passcode.
    private static let fakeNoPasscode = ProcessInfo.processInfo.arguments.contains("--no-device-passcode")
    /// `--device-biometrics` adds enrolled biometrics to the fake passcode.
    private static let fakeBiometrics = ProcessInfo.processInfo.arguments.contains("--device-biometrics")
    #else
    private static let fakePasscode = false
    private static let fakeNoPasscode = false
    private static let fakeBiometrics = false
    #endif

    @Published private(set) var screen = Screen.failed
    @Published private(set) var mode = StoredLock.new
    /// Whether the open app shows its lock settings in place of the start screen.
    @Published private(set) var settingsOpen = false
    /// Catalog key of the message under the PIN field.
    @Published private(set) var notice: String?
    @Published private(set) var busy = false
    /// Whether the device has a passcode: a PIN needs it, as the only way to recover a forgotten one.
    @Published private(set) var deviceSecure = false
    /// A passed device-passcode check opened the settings: the PIN is replaced or cleared without it.
    @Published private(set) var recovered = false
    @Published private(set) var biometrics = Biometrics.absent
    private let store: AppLockStore?
    /// The unlocked session; nil until a PIN entry unlocks the app.
    private var session: AppLockTimer?
    private var expiry: Task<Void, Never>?

    /// Whether a lock is set, so the app is covered whenever it is not active.
    var guarded: Bool { mode == .pin || mode == .biometric }

    /// Whether a settings change skips the current PIN: a fresh OS check opened the settings.
    var authorized: Bool { recovered || mode == .biometric }

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

    private static func deviceOwnerCheck() async -> Bool {
        if Self.fakePasscode { return true }
        let reason = String(localized: "lock.passcode.reason")
        return (try? await LAContext().evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)) ?? false
    }

    private static func biometricState() -> Biometrics {
        if Self.fakeBiometrics { return .ready }
        var error: NSError?
        if LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) { return .ready }
        return error?.code == LAError.biometryNotEnrolled.rawValue ? .notEnrolled : .absent
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

    /// Settings after a device-passcode check: replace the PIN, or turn it off when `newPin` is nil.
    func reset(_ newPin: String?, repeated: String) {
        if let newPin, let rejection = Self.rejection(newPin, repeated) { notice = rejection; return }
        recovered = false
        run { store in
            try store.resetAfterDeviceCredential(newPin)
            return (false, nil)
        }
    }

    /// Device biometrics in place of the PIN, on first launch, while the lock is off, or after a
    /// device-passcode check.
    func useBiometric() {
        recovered = false
        run { store in
            try store.useBiometric()
            return (true, nil)
        }
    }

    /// The OS check from the unlock screen: biometrics or the device passcode. It unlocks the app;
    /// with a PIN it also opens the settings, so a forgotten PIN can be replaced. The identity
    /// Keychain items are never touched; a cancelled check leaves the app locked.
    func deviceUnlock() {
        deviceCheck {
            if self.mode == .pin {
                self.recovered = true
                self.settingsOpen = true
            }
        }
    }

    /// Biometric settings changes need a fresh OS check, as a PIN change asks for the current PIN.
    func openSettings(_ open: Bool) {
        notice = nil
        recovered = false
        if open && mode == .biometric {
            deviceCheck { self.settingsOpen = true }
        } else {
            settingsOpen = open
        }
    }

    /// The background grace starts on `background`; every phase change applies an expired deadline,
    /// so a late return locks while the cover still hides the content.
    func phase(_ phase: ScenePhase) {
        if phase == .active {
            // The user may have just set a passcode in Settings.
            deviceSecure = !Self.fakeNoPasscode &&
                (Self.fakePasscode || LAContext().canEvaluatePolicy(.deviceOwnerAuthentication, error: nil))
            biometrics = Self.biometricState()
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
        recovered = false
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

        case .pin, .biometric:
            screen = session == nil ? .unlock : .open
        }
    }

    private func deviceCheck(_ then: @escaping @MainActor () -> Void) {
        guard deviceSecure, !busy else { return }
        busy = true
        notice = nil
        Task {
            let passed = await Self.deviceOwnerCheck()
            busy = false
            guard passed else { return }
            if session == nil {
                session = AppLockTimer(now: AppLockTimer.uptime())
                watchExpiry()
            }
            then()
            refresh()
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
