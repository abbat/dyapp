import SwiftUI

/// A digits-only PIN entry of at most 64 digits.
struct PinField: View {
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
struct Notice: View {
    let key: String?

    var body: some View {
        if let key {
            Text(LocalizedStringKey(key)).foregroundColor(.red).accessibilityIdentifier("lock-notice")
        }
    }
}

/// First launch: Skip, Set PIN, or device biometrics.
struct SetupView: View {
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
            NoPasscode(deviceSecure: lock.deviceSecure)
            Button("lock.set") { lock.enroll(pin, repeated: repeated) }
                .accessibilityIdentifier("lock-set")
                .disabled(!lock.deviceSecure)
            Button("lock.skip") { lock.skip() }
                .accessibilityIdentifier("lock-skip")
            BiometricOption(lock: lock)
        }
        .disabled(lock.busy)
        .padding()
    }
}

/// Why Set PIN is disabled, if it is.
struct NoPasscode: View {
    let deviceSecure: Bool

    var body: some View {
        if !deviceSecure {
            Text("lock.no.passcode").accessibilityIdentifier("lock-no-passcode")
        }
    }
}

/// Device biometrics in place of the PIN; like a PIN, they need the device passcode.
struct BiometricOption: View {
    @ObservedObject var lock: LockModel

    var body: some View {
        switch lock.biometrics {
        case .ready:
            Button("lock.use.biometric") { lock.useBiometric() }
                .accessibilityIdentifier("lock-biometric")
                .disabled(!lock.deviceSecure)

        case .notEnrolled:
            Text("lock.biometric.none").accessibilityIdentifier("lock-biometric-unavailable")

        case .absent:
            EmptyView()
        }
    }
}

/// App-lock settings in place of the start screen: Set PIN while none is set; with a PIN, Change
/// and Turn off both need the current PIN, unless a fresh OS check opened the screen, as it always
/// does in biometric mode. Biometrics are offered while no PIN guards the choice. Every submit
/// clears the typed PINs.
struct SettingsView: View {
    @ObservedObject var lock: LockModel
    @State private var current = ""
    @State private var pin = ""
    @State private var repeated = ""

    var body: some View {
        let pinSet = lock.mode == .pin
        let askCurrent = pinSet && !lock.authorized
        let verified = !askCurrent || current.count >= LockModel.minPin
        let modeKey = pinSet ? "lock.mode.pin" : lock.mode == .biometric ? "lock.mode.biometric" : "lock.mode.off"
        VStack(spacing: 16) {
            Text(LocalizedStringKey(modeKey)).accessibilityIdentifier("lock-mode")
            if lock.recovered {
                Text("lock.reset").accessibilityIdentifier("lock-reset")
            }
            if askCurrent {
                PinField(title: "lock.pin.current", tag: "lock-pin-current", pin: $current)
            }
            PinField(title: "lock.pin.new", tag: "lock-pin", pin: $pin)
            PinField(title: "lock.pin.repeat", tag: "lock-pin-repeat", pin: $repeated)
            Notice(key: lock.notice)
            NoPasscode(deviceSecure: lock.deviceSecure)
            Button(LocalizedStringKey(pinSet ? "lock.change" : "lock.set")) { submit(pin) }
                .accessibilityIdentifier("lock-set")
                .disabled(!verified || !lock.deviceSecure)
            if lock.guarded {
                Button(LocalizedStringKey(pinSet ? "lock.turn.off" : "lock.turn.off.biometric")) { submit(nil) }
                    .accessibilityIdentifier("lock-off")
                    .disabled(!verified)
            }
            if lock.mode == .off || lock.recovered {
                BiometricOption(lock: lock)
            }
            Button("lock.back") { lock.openSettings(false) }
                .accessibilityIdentifier("lock-back")
        }
        .disabled(lock.busy)
        .padding()
    }

    private func submit(_ newPin: String?) {
        if lock.authorized {
            lock.reset(newPin, repeated: repeated)
        } else if lock.mode == .pin {
            lock.change(current: current, newPin: newPin, repeated: repeated)
        } else {
            lock.enroll(pin, repeated: repeated)
        }
        current = ""
        pin = ""
        repeated = ""
    }
}

/// The only screen of a locked app: the PIN, or the OS check, which also recovers a forgotten PIN
/// and is the only way in under biometrics.
struct UnlockView: View {
    @ObservedObject var lock: LockModel
    @State private var pin = ""

    var body: some View {
        let pinSet = lock.mode == .pin
        VStack(spacing: 16) {
            Text("app.locked").font(.title).accessibilityIdentifier("app-locked")
            if pinSet {
                PinField(title: "lock.pin", tag: "lock-pin", pin: $pin)
            }
            Notice(key: lock.notice)
            if pinSet {
                Button("lock.unlock") {
                    lock.unlock(pin)
                    pin = ""
                }
                .accessibilityIdentifier("lock-unlock")
                .disabled(pin.count < LockModel.minPin)
            }
            Button(LocalizedStringKey(pinSet ? "lock.use.passcode" : "lock.use.biometric")) { lock.deviceUnlock() }
                .accessibilityIdentifier("lock-device-unlock")
                .disabled(!lock.deviceSecure)
            if !lock.deviceSecure {
                Text("lock.recovery.unavailable").accessibilityIdentifier("lock-recovery-unavailable")
            }
        }
        .disabled(lock.busy)
        .padding()
    }
}
