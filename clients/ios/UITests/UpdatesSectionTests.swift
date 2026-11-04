import XCTest

@MainActor
final class UpdatesSectionTests: XCTestCase {
    func testUpdatesStayDisabledUntilTheFeedIsPublished() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        create.tap()
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10), app.debugDescription)
        settings.tap()
        let check = app.buttons["update-check"]
        XCTAssertTrue(app.buttons["settings-network"].waitForExistence(timeout: 10), app.debugDescription)
        reveal(check, in: app)
        XCTAssertTrue(check.exists, app.debugDescription)
        XCTAssertFalse(check.isEnabled)
        XCTAssertTrue(app.staticTexts["update-state"].exists, app.debugDescription)
        app.terminate()
    }
}
