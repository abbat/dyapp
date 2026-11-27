@testable import DYApp
import XCTest

final class AppLockTimerTests: XCTestCase {
    private func unlocked(at now: UInt64) throws -> AppLockSession {
        let session = AppLockSession(now: now)
        XCTAssertTrue(try session.snapshot(at: now).locked)
        try session.authorize(at: now)
        return session
    }

    func testQuickBackgroundReturnKeepsSession() throws {
        var session = try unlocked(at: 1_000)
        try session.phase(active: false, at: 2_000)
        XCTAssertFalse(try session.snapshot(at: 61_999).locked)
        try session.phase(active: true, at: 61_999)
        XCTAssertEqual(try session.snapshot(at: 61_999).remainingMs, 300_000)
    }

    func testBackgroundLocksAfterGraceEvenWithInput() throws {
        var session = try unlocked(at: 1_000)
        try session.phase(active: false, at: 2_000)
        try session.interaction(at: 30_000)
        XCTAssertEqual(try session.snapshot(at: 30_000).remainingMs, 32_000)
        try session.phase(active: true, at: 62_000)
        try session.interaction(at: 62_000)
        XCTAssertTrue(try session.snapshot(at: 62_000).locked)
        XCTAssertNil(try session.snapshot(at: 62_000).remainingMs)
    }

    func testTouchExtendsIdleButCannotReviveSession() throws {
        let session = try unlocked(at: 0)
        try session.interaction(at: 299_999)
        XCTAssertFalse(try session.snapshot(at: 599_998).locked)
        try session.interaction(at: 599_999)
        XCTAssertTrue(try session.snapshot(at: 599_999).locked)
    }

    func testObservedProtectionCannotAuthorizeAndBackgroundContextSurvivesReenable() throws {
        var session = AppLockSession(now: 0)
        XCTAssertTrue(try session.snapshot(at: 0).locked)
        try session.setProtection(false, at: 1)
        XCTAssertFalse(try session.snapshot(at: 1).enabled)
        try session.phase(active: false, at: 2)
        try session.setProtection(true, at: 3)
        XCTAssertTrue(try session.snapshot(at: 3).locked)
        XCTAssertTrue(try session.snapshot(at: 3).obscured)
        XCTAssertThrowsError(try session.authorize(at: 4))
        try session.phase(active: true, at: 5)
        XCTAssertTrue(try session.snapshot(at: 5).locked)
        try session.authorize(at: 5)
        XCTAssertFalse(try session.snapshot(at: 5).locked)
    }

    func testBackwardsClockLocksAndLaterInputCannotReviveIt() throws {
        let session = try unlocked(at: 100)
        try session.interaction(at: 200)
        XCTAssertThrowsError(try session.interaction(at: 199))
        XCTAssertTrue(try session.snapshot(at: 201).locked)
        try session.interaction(at: 202)
        XCTAssertTrue(try session.snapshot(at: 202).locked)
    }

    func testDisabledProtectionCannotAcceptAnUnlock() throws {
        var session = AppLockSession(now: 0)
        try session.setProtection(false, at: 1)
        XCTAssertThrowsError(try session.authorize(at: 2))
        XCTAssertNil(try session.snapshot(at: 2).remainingMs)
    }
}
