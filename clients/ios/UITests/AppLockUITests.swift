import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class AppLockUITests: XCTestCase {
    private let pin = "123456"

    func testSkipOpensStartScreen() {
        let app = launch(["--app-lock-new"])
        app.buttons["lock-skip"].tap()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 10))
        app.terminate()
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 10))
        app.terminate()
    }

    func testSetupRejectsShortAndMismatchedPins() {
        // A secure field clears itself when focused again, so each case starts a fresh launch.
        var app = launch(["--app-lock-new"])
        enter(app, "lock-pin", "12345")
        app.buttons["lock-set"].tap()
        XCTAssertEqual(notice(app), "The PIN needs at least 6 digits.")
        app.terminate()
        app = launch(["--app-lock-new"])
        enter(app, "lock-pin", pin)
        enter(app, "lock-pin-repeat", "654321")
        app.buttons["lock-set"].tap()
        XCTAssertEqual(notice(app), "The PINs do not match.")
        XCTAssertFalse(app.staticTexts["peer-id"].exists)
        app.terminate()
    }

    /// A PIN locks every fresh start; nothing private renders before the right PIN.
    func testPinLocksEveryStart() {
        let app = launch(["--app-lock-new"])
        enter(app, "lock-pin", pin)
        enter(app, "lock-pin-repeat", pin)
        app.buttons["lock-set"].tap()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 30))
        app.terminate()
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.staticTexts["app-locked"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["peer-id"].exists)
        enter(app, "lock-pin", "000000")
        app.buttons["lock-unlock"].tap()
        XCTAssertEqual(notice(app), "Wrong PIN.")
        XCTAssertFalse(app.staticTexts["peer-id"].exists)
        enter(app, "lock-pin", pin)
        app.buttons["lock-unlock"].tap()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 30))
        app.terminate()
    }

    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = arguments
        app.launch()
        XCTAssertTrue(app.buttons["lock-skip"].waitForExistence(timeout: 10))
        return app
    }

    private func enter(_ app: XCUIApplication, _ field: String, _ text: String) {
        let input = app.secureTextFields[field]
        XCTAssertTrue(input.waitForExistence(timeout: 10))
        input.tap()
        input.typeText(text)
    }

    private func notice(_ app: XCUIApplication) -> String {
        let notice = app.staticTexts["lock-notice"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30))
        return notice.label
    }
}
