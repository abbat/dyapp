import Combine
@testable import DYApp
import XCTest

private struct LockReadFailure: Error {}

/// Tests own the read transition explicitly instead of racing storage or the main run loop.
private final class HeldLockVault: AppLockVault, @unchecked Sendable {
    let entered = NativeTestSignal()
    private let release = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var held = true
    private var failRead = false
    private var constructions = 0
    private let record: Data?

    init(record: Data? = Data([1, 2, 0])) { self.record = record }

    func makeStore() -> AppLockStore {
        lock.lock()
        defer { lock.unlock() }
        constructions += 1
        XCTAssertEqual(constructions, 1, "Retry must retain an already constructed native handle")
        return AppLockStore(vault: self)
    }

    func resume(failRead: Bool = false) {
        lock.lock()
        self.failRead = failRead
        held = false
        lock.unlock()
        release.signal()
    }

    func load() throws -> Data? {
        XCTAssertFalse(Thread.isMainThread, "Protected storage must not block UI")
        lock.lock()
        let waitForRelease = held
        lock.unlock()
        entered.signal()
        if waitForRelease, release.wait(timeout: .now() + 10) != .success {
            XCTFail("The test did not release the owned read")
            throw LockReadFailure()
        }
        lock.lock()
        defer { lock.unlock() }
        if failRead { throw LockReadFailure() }
        return record
    }

    func save(_ data: Data) throws { XCTFail("Reading must not rewrite protected bytes") }
    func delete() throws { XCTFail("Reading must not erase protected bytes") }
}

@MainActor
final class AppLockReadinessTests: XCTestCase {
    func testPendingReadKeepsPrivateContentAbsentUntilActualCompletion() async {
        let vault = HeldLockVault()
        let store = AppLockStore(vault: vault)
        let model = LockModel { store }
        let finished = NativeTestSignal()
        let observation = model.$screen.sink { if $0 == .unlock { finished.signal() } }
        defer { observation.cancel() }
        let entered = await vault.entered.wait()
        XCTAssertTrue(entered)
        XCTAssertEqual(model.screen, .loading)
        XCTAssertTrue(model.busy)
        vault.resume()
        let completed = await finished.wait()
        XCTAssertTrue(completed)
        XCTAssertEqual(model.screen, .unlock)
        XCTAssertEqual(model.mode, .biometric)
        XCTAssertFalse(model.busy)
    }

    func testReadFailureStaysProtectedAndRetryUsesTheExistingStore() async {
        let vault = HeldLockVault()
        let model = LockModel { vault.makeStore() }
        let failed = NativeTestSignal()
        let ready = NativeTestSignal()
        let observation = model.$screen.sink {
            if $0 == .failed { failed.signal() }
            if $0 == .unlock { ready.signal() }
        }
        defer { observation.cancel() }
        let entered = await vault.entered.wait()
        XCTAssertTrue(entered)
        vault.resume(failRead: true)
        let failureDelivered = await failed.wait()
        XCTAssertTrue(failureDelivered)
        XCTAssertEqual(model.screen, .failed)
        vault.resume()
        model.retry()
        XCTAssertEqual(model.screen, .loading)
        let completed = await ready.wait()
        XCTAssertTrue(completed)
        XCTAssertEqual(model.screen, .unlock)
    }

    func testOnlyConfirmedAbsenceOrOffCanChooseSetupOrOpen() async {
        for (record, expected): (Data?, LockModel.Screen) in [
            (nil, .setup), (Data([1, 0, 0]), .open),
        ] {
            let vault = HeldLockVault(record: record)
            let model = LockModel { vault.makeStore() }
            let ready = NativeTestSignal()
            let observation = model.$screen.sink { if $0 == expected { ready.signal() } }
            let entered = await vault.entered.wait()
            XCTAssertTrue(entered)
            XCTAssertEqual(model.screen, .loading)
            vault.resume()
            let completed = await ready.wait()
            XCTAssertTrue(completed)
            XCTAssertEqual(model.screen, expected)
            observation.cancel()
        }
    }

    func testDisposedModelDoesNotRetainOrRedirectItsLateRead() async {
        let vault = HeldLockVault()
        let store = AppLockStore(vault: vault)
        var old: LockModel? = LockModel { store }
        weak var retired = old
        let entered = await vault.entered.wait()
        XCTAssertTrue(entered)
        old = nil
        XCTAssertNil(retired)
        let current = LockModel { throw LockReadFailure() }
        let failed = NativeTestSignal()
        let observation = current.$screen.sink { if $0 == .failed { failed.signal() } }
        defer { observation.cancel() }
        vault.resume()
        let completed = await failed.wait()
        XCTAssertTrue(completed)
        XCTAssertEqual(current.screen, .failed)
    }
}
