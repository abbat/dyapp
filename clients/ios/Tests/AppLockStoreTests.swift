import CryptoKit
@testable import DYApp
import XCTest

private final class MemoryVault: AppLockVault {
    var data: Data?
    func load() throws -> Data? { data }
    func save(_ data: Data) throws { self.data = data }
    func delete() throws { data = nil }
}

private struct Crash: Error {}

final class AppLockStoreTests: XCTestCase {
    private let vault = MemoryVault()

    // A fresh store over the same vault stands for a process restart.
    private func store(crash: Bool = false) -> AppLockStore {
        // 49 bytes like the Rust verifier, without its Argon2id cost.
        let hash = { (pin: String) in Data(SHA256.hash(data: Data(pin.utf8))) + Data(count: 17) }
        let fake = AppPinVerifier(hash: hash) { pinValue, record in
            if crash { throw Crash() }
            return hash(pinValue) == record
        }
        return AppLockStore(vault: vault, pin: fake)
    }

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

    func testAttemptIsCountedBeforeVerifying() throws {
        try store().enroll("123456")
        for _ in 1...5 { XCTAssertThrowsError(try store(crash: true).attempt("123456")) }
        XCTAssertEqual(try store().attempt("123456"), .deviceCredentialRequired)
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
