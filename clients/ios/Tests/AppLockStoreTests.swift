@testable import DYApp
import XCTest

private final class MemoryVault: AppLockVault {
    var data: Data?
    var failReset = false
    var failRead = false
    func load() throws -> Data? {
        if failRead { throw Crash() }
        return data
    }
    func save(_ data: Data) throws {
        if failReset, data.count >= 3, data[2] == 0 { throw Crash() }
        self.data = data
    }
    func delete() throws { data = nil }
}

private struct Crash: Error {}

final class AppLockStoreTests: XCTestCase {
    private let vault = MemoryVault()

    // A fresh store over the same vault stands for a process restart.
    private func store() -> AppLockStore { AppLockStore(vault: vault) }

    func testCorrectPinUnlocksAndClearsFailures() throws {
        XCTAssertEqual(try store().mode(), .new)
        try store().enroll("123456")
        XCTAssertEqual(try store().attempt("000000"), .wrong)
        XCTAssertEqual(try store().attempt("123456"), .unlocked)
        for _ in 1...4 { XCTAssertEqual(try store().attempt("000000"), .wrong) }
        XCTAssertEqual(try store().mode(), .pin)
        XCTAssertThrowsError(try store().enroll("654321"))
    }

    func testFiveFailuresRequireDeviceCredential() throws {
        try store().enroll("123456")
        for _ in 1...4 { XCTAssertEqual(try store().attempt("000000"), .wrong) }
        XCTAssertEqual(try store().attempt("000000"), .deviceCredentialRequired)
        XCTAssertEqual(try store().attempt("123456"), .deviceCredentialRequired)
        try store().resetAfterDeviceCredential("654321")
        XCTAssertEqual(try store().attempt("654321"), .unlocked)
    }

    func testFailedResetCannotEraseConsumedAttemptsAcrossHandles() throws {
        try store().enroll("123456")
        vault.failReset = true
        for _ in 1...5 { XCTAssertThrowsError(try store().attempt("123456")) }
        XCTAssertEqual(try store().attempt("123456"), .deviceCredentialRequired)
    }

    func testAttemptIsCountedBeforeVerifying() throws {
        try store().enroll("123456")
        // The real verifier rejects invalid input only after core persists the attempt.
        for _ in 1...5 { XCTAssertThrowsError(try store().attempt("")) }
        XCTAssertEqual(try store().attempt("123456"), .deviceCredentialRequired)
    }

    func testProtectedReadFailureCannotBecomeFreshSetup() throws {
        try store().enroll("123456")
        let saved = vault.data
        vault.failRead = true
        XCTAssertThrowsError(try store().mode())
        XCTAssertThrowsError(try store().enroll("654321"))
        XCTAssertEqual(vault.data, saved)
    }

    func testLegacyKeychainRecordReopensWithoutMigration() throws {
        let verifier = try hashAppPin(pin: "123456")
        let legacy = Data([1, 1, 0]) + verifier
        vault.data = legacy
        XCTAssertEqual(try store().mode(), .pin)
        XCTAssertEqual(vault.data, legacy, "Reading must not rewrite existing lock bytes")
        XCTAssertEqual(try store().attempt("123456"), .unlocked)
        XCTAssertEqual(vault.data, legacy)
    }

    func testInitialChoicesCannotReplaceAnEnabledLock() throws {
        try store().useBiometric()
        let biometric = vault.data
        XCTAssertThrowsError(try store().enroll("123456"))
        XCTAssertEqual(vault.data, biometric)
        try store().resetAfterDeviceCredential("123456")
        let pin = vault.data
        XCTAssertThrowsError(try store().useBiometric())
        XCTAssertEqual(vault.data, pin)
        try store().useBiometric(afterDeviceCredential: true)
        XCTAssertEqual(try store().mode(), .biometric)
    }

    func testCorruptRecordKeepsTheAppLocked() throws {
        try store().enroll("123456")
        vault.data?[2] = 9
        XCTAssertThrowsError(try store().mode())
        vault.data = Data([1, 1, 0])
        XCTAssertThrowsError(try store().attempt("123456"))
        vault.data = Data([2, 0, 0])
        XCTAssertThrowsError(try store().mode())
    }

    func testChoicesReplaceEachOther() throws {
        try store().skip()
        XCTAssertEqual(try store().mode(), .off)
        XCTAssertThrowsError(try store().resetAfterDeviceCredential(nil))
        try store().useBiometric()
        try store().skip()
        XCTAssertEqual(try store().mode(), .biometric)
        try store().resetAfterDeviceCredential("123456")
        XCTAssertEqual(try store().mode(), .pin)
        XCTAssertEqual(try store().change(current: "000000", newPin: nil), .wrong)
        XCTAssertEqual(try store().change(current: "123456", newPin: nil), .unlocked)
        XCTAssertEqual(try store().mode(), .off)
    }
}
