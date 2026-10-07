import XCTest

final class LaunchTests: XCTestCase {
    func testLaunchRenderAndTerminate() {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.staticTexts["DYApp"].waitForExistence(timeout: 10))
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertEqual(ready.label, "Ready")
        XCTAssertTrue(ready.isHittable)
        app.terminate()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    func testHiddenReadyDoesNotSatisfyLaunchContract() {
        let app = XCUIApplication()
        app.launchArguments = ["--break-ui"]
        app.launch()
        XCTAssertTrue(app.staticTexts["DYApp"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
        app.terminate()
    }
}
