@testable import DYApp
import Foundation

/// Arm only after native worker construction; this fixture has no competing identity readers.
final class IdentityAdmissionKeys: MobileSlotKeys, @unchecked Sendable {
    let entered = NativeTestSignal("Native identity admission")
    let finished = NativeTestSignal("Actual native worker completion")
    private let native = NativeSlotKeys(legacy: .forSlot(UUID()))
    private let lock = NSLock()
    private let released = DispatchSemaphore(value: 0)
    private var armed = false

    func arm() {
        lock.lock()
        armed = true
        lock.unlock()
    }

    func resume() { released.signal() }

    private func takeGate() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        let held = armed
        armed = false
        return held
    }

    func contains(slotId: String) throws -> Bool { try native.contains(slotId: slotId) }

    func openWrapped(slotId: String, wrapped: Data) throws -> IdentityKey? {
        if takeGate() {
            entered.signal()
            guard released.wait(timeout: .now() + 10) == .success else {
                throw WrapError.Failed(reason: "The test did not release native admission")
            }
        }
        return try native.openWrapped(slotId: slotId, wrapped: wrapped)
    }

    func wrap(slotId: String, key: IdentityKey) throws -> Data {
        try native.wrap(slotId: slotId, key: key)
    }

    func wrapExisting(slotId: String, key: IdentityKey) throws -> Data {
        try native.wrapExisting(slotId: slotId, key: key)
    }

    func erase(slotId: String) throws { try native.erase(slotId: slotId) }
}
