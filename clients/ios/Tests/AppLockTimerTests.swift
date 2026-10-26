@testable import DYApp
import XCTest

final class AppLockTimerTests: XCTestCase {
    func testQuickBackgroundReturnKeepsSession() {
        var timer = AppLockTimer(now: 1_000)
        timer.background(at: 2_000)
        XCTAssertFalse(timer.expired(at: 61_999))
        timer.foreground(at: 61_999)
        XCTAssertEqual(timer.deadline, 361_999)
    }

    func testBackgroundLocksAfterGraceEvenWithInput() {
        var timer = AppLockTimer(now: 1_000)
        timer.background(at: 2_000)
        timer.interaction(at: 30_000)
        XCTAssertEqual(timer.deadline, 62_000)
        timer.foreground(at: 62_000)
        timer.interaction(at: 62_000)
        XCTAssertTrue(timer.expired(at: 62_000))
    }

    func testTouchExtendsIdleButCannotReviveSession() {
        var timer = AppLockTimer(now: 0)
        timer.interaction(at: 299_999)
        XCTAssertFalse(timer.expired(at: 599_998))
        timer.interaction(at: 599_999)
        XCTAssertTrue(timer.expired(at: 599_999))
    }
}
