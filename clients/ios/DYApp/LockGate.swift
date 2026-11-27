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
        case .loading:
            ProgressView()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .accessibilityIdentifier("lock-loading")

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
            VStack {
                Text("app.locked").accessibilityIdentifier("app-locked")
                Button("identity.retry") { lock.retry() }.accessibilityIdentifier("lock-retry")
            }
        }
    }
}

/// Lock state for the gate. PIN work runs off the main thread: the Argon2id KDF takes 64 MiB.
@MainActor
final class LockModel: ObservableObject {
    /// What the gate shows.
    enum Screen { case loading, setup, unlock, open, failed }

    /// Whether device biometrics can be offered in place of the PIN.
    enum Biometrics { case ready, notEnrolled, absent }

    /// Shortest accepted PIN; the Rust verifier enforces 6 to 64 digits.
    static let minPin = 6

    #if DEBUG
        /// Simulators have no passcode: UI tests pass `--device-passcode` to stand in for one that
        /// always checks out.
        private static let fakePasscode = ProcessInfo.processInfo.arguments.contains(
            "--device-passcode")
        /// Force the missing-passcode path even when a CI simulator has a passcode.
        private static let fakeNoPasscode = ProcessInfo.processInfo.arguments.contains(
            "--no-device-passcode")
        /// `--device-biometrics` adds enrolled biometrics to the fake passcode.
        private static let fakeBiometrics = ProcessInfo.processInfo.arguments.contains(
            "--device-biometrics")
    #else
        private static let fakePasscode = false
        private static let fakeNoPasscode = false
        private static let fakeBiometrics = false
    #endif

    @Published private(set) var screen = Screen.loading
    @Published private(set) var mode = StoredLock.new
    /// Whether the open app shows its lock settings in place of the start screen.
    @Published private(set) var settingsOpen = false
    /// Catalog key of the message under the PIN field.
    @Published private(set) var notice: String?
    @Published private(set) var busy = true
    /// Whether the device has a passcode: a PIN needs it, as the only way to recover a forgotten one.
    @Published private(set) var deviceSecure = false
    /// A passed device-passcode check opened the settings: the PIN is replaced or cleared without it.
    @Published private(set) var recovered = false
    @Published private(set) var biometrics = Biometrics.absent
    private var store: AppLockStore?
    private let makeStore: @Sendable () throws -> AppLockStore
    private var operation = UUID()
    /// Retains lifecycle events while locked; credential completion cannot reset the context.
    private var session = AppLockSession(now: AppLockSession.uptime())
    private var expiry: Task<Void, Never>?

    /// Whether a lock is set, so the app is covered whenever it is not active.
    var guarded: Bool { mode == .pin || mode == .biometric }

    /// Whether a settings change skips the current PIN: a fresh OS check opened the settings.
    var authorized: Bool { recovered || mode == .biometric }

    init(makeStore: @escaping @Sendable () throws -> AppLockStore = { try AppLockIO.makeStore() }) {
        self.makeStore = makeStore
        readMode()
    }

    /// The gate's reading of a counted PIN attempt; five failures leave only the
    /// device-credential notice.
    nonisolated private static func outcome(_ attempt: PinAttempt) -> (
        unlocked: Bool, notice: String?
    ) {
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
        return
            (try? await LAContext().evaluatePolicy(
                .deviceOwnerAuthentication, localizedReason: reason)) ?? false
    }

    private static func biometricState() -> Biometrics {
        if Self.fakeBiometrics { return .ready }
        var error: NSError?
        if LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) {
            return .ready
        }
        return error?.code == LAError.biometryNotEnrolled.rawValue ? .notEnrolled : .absent
    }

    func retry() {
        guard !busy else { return }
        readMode()
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
        if let newPin, let rejection = Self.rejection(newPin, repeated) {
            notice = rejection; return
        }
        run { store in Self.outcome(try store.change(current: current, newPin: newPin)) }
    }

    /// Settings after a device-passcode check: replace the PIN, or turn it off when `newPin` is nil.
    func reset(_ newPin: String?, repeated: String) {
        if let newPin, let rejection = Self.rejection(newPin, repeated) {
            notice = rejection; return
        }
        recovered = false
        run { store in
            try store.resetAfterDeviceCredential(newPin)
            return (false, nil)
        }
    }

    /// Device biometrics in place of the PIN, on first launch, while the lock is off, or after a
    /// device-passcode check.
    func useBiometric() {
        let authenticated = recovered
        recovered = false
        run { store in
            try store.useBiometric(afterDeviceCredential: authenticated)
            return (true, nil)
        }
    }

    /// The OS check from the unlock screen: biometrics or the device passcode. It unlocks the app;
    /// with a PIN it also opens the settings, so a forgotten PIN can be replaced. The identity
    /// Keychain items are never touched; a cancelled check leaves the app locked.
    func deviceUnlock() {
        deviceCheck { [weak self] in
            if self?.mode == .pin {
                self?.recovered = true
                self?.settingsOpen = true
            }
        }
    }

    /// Biometric settings changes need a fresh OS check, as a PIN change asks for the current PIN.
    func openSettings(_ open: Bool) {
        notice = nil
        recovered = false
        if open && mode == .biometric {
            deviceCheck { [weak self] in self?.settingsOpen = true }
        } else {
            settingsOpen = open
        }
    }

    /// The background grace starts on `background`; every phase change applies an expired deadline,
    /// so a late return locks while the cover still hides the content.
    func phase(_ phase: ScenePhase) {
        if phase == .active {
            // The user may have just set a passcode in Settings.
            deviceSecure =
                !Self.fakeNoPasscode
                && (Self.fakePasscode
                    || LAContext().canEvaluatePolicy(.deviceOwnerAuthentication, error: nil))
            biometrics = Self.biometricState()
            watchTouches()
            try? session.phase(active: true, at: AppLockSession.uptime())
        } else if phase == .background {
            try? session.phase(active: false, at: AppLockSession.uptime())
        }
        expireIfDue()
        watchExpiry()
    }

    // ponytail: touches on the app's windows only; keyboard typing lives in a separate system
    // window, so count text edits too once a screen takes long text input.
    private func watchTouches() {
        let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
            .flatMap(\.windows)
        for window in windows
        where !(window.gestureRecognizers ?? []).contains(where: { $0 is TouchWatcher }) {
            window.addGestureRecognizer(
                TouchWatcher { [weak self] in
                    try? self?.session.interaction(at: AppLockSession.uptime())
                    self?.expireIfDue()
                    self?.watchExpiry()
                })
        }
    }

    /// Schedules the core's remaining duration; actual events replace this pending wakeup.
    private func watchExpiry() {
        expiry?.cancel()
        guard guarded else { return }
        guard let remaining = (try? session.snapshot(at: AppLockSession.uptime()))?.remainingMs
        else {
            expireIfDue()
            return
        }
        expiry = Task { [weak self] in
            do { try await Task.sleep(nanoseconds: remaining * 1_000_000) } catch { return }
            guard !Task.isCancelled else { return }
            self?.expireIfDue()
            self?.watchExpiry()
        }
    }

    private func expireIfDue() {
        guard guarded else { return }
        guard let snapshot = try? session.snapshot(at: AppLockSession.uptime()) else {
            screen = .failed
            return
        }
        guard snapshot.locked else { return }
        if case .failed = screen { return }
        if case .loading = screen { return }
        showLocked()
    }

    private func showLocked() {
        // Repeated locked observations must retain wrong-PIN and device-credential feedback.
        if case .open = screen { notice = nil }
        recovered = false
        settingsOpen = false
        screen = .unlock
    }

    @discardableResult
    private func refresh(_ result: Result<StoredLock, Error>) -> Bool {
        guard let mode = try? result.get() else { screen = .failed; return false }
        self.mode = mode
        do {
            try session.setProtection(guarded, at: AppLockSession.uptime())
            switch mode {
            case .new:
                screen = .setup

            case .off:
                screen = .open

            case .pin, .biometric:
                if try session.snapshot(at: AppLockSession.uptime()).locked {
                    showLocked()
                } else {
                    screen = .open
                }
            }
            return true
        } catch {
            screen = .failed
            return false
        }
    }

    deinit { expiry?.cancel() }
}

private extension LockModel {
    func deviceCheck(_ then: @escaping @MainActor @Sendable () -> Void) {
        guard deviceSecure, !busy else { return }
        busy = true
        notice = nil
        let ticket = UUID()
        operation = ticket
        Task { [weak self] in
            let passed = await Self.deviceOwnerCheck()
            guard let self, self.operation == ticket else { return }
            self.busy = false
            guard passed, let store = self.store else { return }
            self.performOwned(
                { try store.mode() },
                apply: { model, result in
                    guard model.refresh(result), model.authorize() else { return }
                    then()
                    model.refresh(result)
                    model.watchExpiry()
                })
        }
    }

    func readMode() {
        screen = .loading
        let current = store
        let factory = makeStore
        performOwned(
            {
                let store = try current ?? factory()
                return AppLockLoad(store: store, mode: Result { try store.mode() })
            },
            apply: { model, result in
                switch result {
                case let .success(load):
                    model.store = load.store
                    model.refresh(load.mode)
                    model.watchExpiry()

                case .failure:
                    model.screen = .failed
                }
            })
    }

    func performOwned<T>(
        _ work: @escaping @Sendable () throws -> T,
        apply: @escaping @MainActor @Sendable (LockModel, Result<T, Error>) -> Void
    ) {
        let ticket = UUID()
        operation = ticket
        busy = true
        AppLockIO.perform(work) { [weak self] result in
            guard let self, self.operation == ticket else { return }
            self.busy = false
            apply(self, result)
        }
    }

    func authorize() -> Bool {
        guard guarded else { return true }
        do {
            try session.authorize(at: AppLockSession.uptime())
            return true
        } catch {
            notice = "lock.failed"
            expireIfDue()
            return false
        }
    }

    func run(
        _ action: @escaping @Sendable (AppLockStore) throws -> (unlocked: Bool, notice: String?)
    ) {
        guard let store, !busy else { return }
        notice = nil
        performOwned(
            {
                AppLockChange(
                    outcome: Result { try action(store) }, mode: Result { try store.mode() })
            },
            apply: { model, result in
                guard let change = try? result.get(), model.refresh(change.mode) else { return }
                switch change.outcome {
                case let .success(outcome):
                    if outcome.unlocked, !model.authorize() { return }
                    model.notice = outcome.notice

                case .failure:
                    model.notice = "lock.failed"
                }
                model.refresh(change.mode)
                model.watchExpiry()
            })
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
