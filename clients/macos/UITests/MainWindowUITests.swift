import XCTest

final class MainWindowUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testAMovedWindowComesBackAfterRelaunch() {
        let root = UUID().uuidString
        let app = launched(root: root)
        let window = shownWindow(app)
        let start = window.frame
        let bar = window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: 12))
        bar.press(forDuration: 0.3, thenDragTo: bar.withOffset(CGVector(dx: 60, dy: 40)))
        let moved = window.frame
        XCTAssertNotEqual(moved.origin, start.origin, app.debugDescription)
        // AppKit hands the saved frame to the defaults daemon just after the move.
        Thread.sleep(forTimeInterval: 1)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownWindow(app).frame, moved, app.debugDescription)
        app.terminate()
    }

    func testAnOffScreenWindowReturnsToADisplayAtItsSize() {
        let root = UUID().uuidString
        let app = launched(root: root, defaults: ["-NSWindow Frame main-\(root)", "-9000 -9000 520 640 0 0 1024 768"])
        let frame = shownWindow(app).frame
        XCTAssertEqual(frame.size, CGSize(width: 520, height: 640), app.debugDescription)
        let display = XCUIScreen.main.screenshot().image.size
        XCTAssertTrue(CGRect(origin: .zero, size: display).intersects(frame), "\(frame) on \(display)")
        app.terminate()
    }

    func testCloseHidesTheWindowAndTheMenuBarBringsItBackOrQuits() {
        let app = launched(root: UUID().uuidString, defaults: ["-menuBarIcon", "YES"])
        closeAndReopen(app)
        XCTAssertEqual(app.state, .runningForeground, app.debugDescription)
        quitFromMenuBar(app)
    }

    func testCloseToMenuBarAlsoComesBack() {
        let app = launched(
            root: UUID().uuidString, defaults: ["-menuBarIcon", "YES", "-closeToMenuBar", "YES"]
        )
        closeAndReopen(app)
        quitFromMenuBar(app)
    }

    func testTurningOffTheIconTurnsOffCloseToMenuBar() {
        let app = launched(root: UUID().uuidString)
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        create.click()
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10), app.debugDescription)
        settings.click()
        let show = app.descendants(matching: .any)["tray-show"]
        let close = app.descendants(matching: .any)["tray-close"]
        XCTAssertTrue(show.waitForExistence(timeout: 10), app.debugDescription)
        reveal(close, in: app)
        close.click()
        XCTAssertEqual(close.value as? Int, 1, app.debugDescription)
        show.click()
        XCTAssertEqual(close.value as? Int, 0, app.debugDescription)
        XCTAssertFalse(close.isEnabled, app.debugDescription)
        XCTAssertFalse(app.statusItems["menubar-icon"].exists, app.debugDescription)
        // Other tests of this run share these defaults.
        show.click()
        app.terminate()
    }

    /// Put `-Key value` defaults before the slot root; see `LaunchTests.isolatedApp`.
    private func launched(root: String, defaults: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = defaults + ["--ui-test-slot-root=\(root)"]
        app.launch()
        return app
    }

    private func shownWindow(_ app: XCUIApplication) -> XCUIElement {
        let window = app.windows.firstMatch
        XCTAssertTrue(window.waitForExistence(timeout: 10), app.debugDescription)
        return window
    }

    private func closeAndReopen(_ app: XCUIApplication) {
        shownWindow(app).buttons[XCUIIdentifierCloseWindow].click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == false"), object: app.windows.firstMatch
        ),
        ], timeout: 10), .completed, app.debugDescription)
        openMenuBarMenu(app)
        app.menuItems["menubar-open"].click()
        _ = shownWindow(app)
    }

    private func quitFromMenuBar(_ app: XCUIApplication) {
        openMenuBarMenu(app)
        app.menuItems["menubar-quit"].click()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10), app.debugDescription)
    }

    private func openMenuBarMenu(_ app: XCUIApplication) {
        let icon = app.statusItems["menubar-icon"]
        XCTAssertTrue(icon.waitForExistence(timeout: 10), app.debugDescription)
        icon.click()
    }
}
