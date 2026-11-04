@testable import DYApp
import XCTest

final class AppTests: XCTestCase {
    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.macos")
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String, "0.0.1")
    }

    func testASavedFrameStaysOnlyWhereItCanBeReached() {
        let primary = CGRect(x: 0, y: 0, width: 1000, height: 800)
        let right = CGRect(x: 1000, y: 0, width: 500, height: 800)
        let onBoth = CGRect(x: 800, y: 100, width: 400, height: 500)
        XCTAssertEqual(WindowPlacement.restored(onBoth, on: [primary, right]), onBoth)
        // A display went away: less than a quarter of the window is left.
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 950, y: 100, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        // Most of the window shows, but its title bar is above the display.
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 100, y: 400, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        XCTAssertEqual(
            WindowPlacement.restored(CGRect(x: 5000, y: 0, width: 2000, height: 1000), on: [primary]),
            primary
        )
    }
}
