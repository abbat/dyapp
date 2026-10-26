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
        // A return within the 60 s background grace keeps the session.
        XCUIDevice.shared.press(.home)
        app.activate()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["app-locked"].exists)
        app.terminate()
    }

    /// Settings set a PIN, and turn it off only behind the current one.
    func testSettingsSetAndTurnOffPin() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--device-passcode"]
        app.launch()
        XCTAssertTrue(app.buttons["lock-settings"].waitForExistence(timeout: 10))
        app.buttons["lock-settings"].tap()
        XCTAssertTrue(label(app.staticTexts["lock-mode"], "App lock: off"))
        enter(app, "lock-pin", pin)
        enter(app, "lock-pin-repeat", pin)
        app.buttons["lock-set"].tap()
        XCTAssertTrue(label(app.staticTexts["lock-mode"], "App lock: PIN"))
        enter(app, "lock-pin-current", "000000")
        app.buttons["lock-off"].tap()
        XCTAssertEqual(notice(app), "Wrong PIN.")
        XCTAssertEqual(app.staticTexts["lock-mode"].label, "App lock: PIN")
        enter(app, "lock-pin-current", pin)
        app.buttons["lock-off"].tap()
        XCTAssertTrue(label(app.staticTexts["lock-mode"], "App lock: off"))
        app.terminate()
        app.launchArguments = []
        app.launch()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 10))
        app.terminate()
    }

    /// A changed PIN replaces the old one at the next start.
    func testSettingsChangePin() {
        let app = launch(["--app-lock-new"])
        enter(app, "lock-pin", pin)
        enter(app, "lock-pin-repeat", pin)
        app.buttons["lock-set"].tap()
        XCTAssertTrue(app.buttons["lock-settings"].waitForExistence(timeout: 30))
        app.buttons["lock-settings"].tap()
        enter(app, "lock-pin-current", pin)
        enter(app, "lock-pin", "654321")
        enter(app, "lock-pin-repeat", "654321")
        app.buttons["lock-set"].tap()
        // The whole form is disabled while the KDF runs.
        let done = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: app.buttons["lock-back"])
        XCTAssertEqual(XCTWaiter().wait(for: [done], timeout: 30), .completed)
        XCTAssertFalse(app.staticTexts["lock-notice"].exists)
        app.terminate()
        app.launchArguments = []
        app.launch()
        enter(app, "lock-pin", pin)
        app.buttons["lock-unlock"].tap()
        XCTAssertEqual(notice(app), "Wrong PIN.")
        enter(app, "lock-pin", "654321")
        app.buttons["lock-unlock"].tap()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 30))
        app.terminate()
    }

    /// Without a device passcode, which simulators lack, no PIN can be set.
    func testSetPinNeedsDevicePasscode() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-new"]
        app.launch()
        XCTAssertTrue(app.staticTexts["lock-no-passcode"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["lock-set"].isEnabled)
        app.terminate()
    }

    /// The device passcode replaces a forgotten PIN and keeps the identity.
    func testPasscodeRecoversForgottenPin() {
        let app = launch(["--app-lock-new"])
        enter(app, "lock-pin", pin)
        enter(app, "lock-pin-repeat", pin)
        app.buttons["lock-set"].tap()
        XCTAssertTrue(app.staticTexts["peer-id"].waitForExistence(timeout: 30))
        let peer = app.staticTexts["peer-id"].label
        app.terminate()
        app.launchArguments = ["--device-passcode"]
        app.launch()
        XCTAssertTrue(app.buttons["lock-passcode"].waitForExistence(timeout: 10))
        app.buttons["lock-passcode"].tap()
        XCTAssertTrue(app.staticTexts["lock-reset"].waitForExistence(timeout: 10))
        enter(app, "lock-pin", "654321")
        enter(app, "lock-pin-repeat", "654321")
        app.buttons["lock-set"].tap()
        let done = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: app.buttons["lock-back"])
        XCTAssertEqual(XCTWaiter().wait(for: [done], timeout: 30), .completed)
        // Once replaced, the next change asks for the new PIN again.
        XCTAssertTrue(app.secureTextFields["lock-pin-current"].exists)
        app.buttons["lock-back"].tap()
        XCTAssertTrue(label(app.staticTexts["peer-id"], peer))
        app.terminate()
        app.launchArguments = []
        app.launch()
        enter(app, "lock-pin", "654321")
        app.buttons["lock-unlock"].tap()
        XCTAssertTrue(label(app.staticTexts["peer-id"], peer))
        app.terminate()
    }

    private func launch(_ arguments: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = arguments + ["--device-passcode"]
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

    private func label(_ element: XCUIElement, _ text: String) -> Bool {
        let shown = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", text), object: element)
        return XCTWaiter().wait(for: [shown], timeout: 30) == .completed
    }

    private func notice(_ app: XCUIApplication) -> String {
        let notice = app.staticTexts["lock-notice"]
        XCTAssertTrue(notice.waitForExistence(timeout: 30))
        return notice.label
    }
}
