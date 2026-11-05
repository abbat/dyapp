import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class MainWindowUITests: XCTestCase {
    func testAMovedWindowComesBackAfterRelaunch() {
        let root = UUID().uuidString
        // Small enough to stay on a small screen once moved: macOS pulls back a window that overflows.
        let app = launched(root: root, defaults: ["-NSWindow Frame main-\(root)", "100 120 520 600 0 0 1024 768"])
        let window = shownWindow(app)
        let start = window.frame
        let bar = window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0)).withOffset(CGVector(dx: 0, dy: 12))
        bar.press(forDuration: 0.3, thenDragTo: bar.withOffset(CGVector(dx: 60, dy: 40)))
        let moved = window.frame
        XCTAssertNotEqual(moved.origin, start.origin, app.debugDescription)
        // A killed app can lose a default it has not flushed yet; a person quits with Cmd+Q.
        app.typeKey("q", modifierFlags: .command)
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10), app.debugDescription)
        // A launch argument outranks the frame the app saved, so the relaunch must not pass the old one.
        app.launchArguments = ["--ui-test-slot-root=\(root)"]
        app.launch()
        XCTAssertEqual(shownWindow(app).frame, moved, "started at \(start)\n\(app.debugDescription)")
        app.terminate()
    }

    func testAnOffScreenWindowReturnsToADisplayAtItsSize() {
        let root = UUID().uuidString
        // A value starting with a dash would read as the next argument's key, so off screen is far positive.
        let app = launched(root: root, defaults: ["-NSWindow Frame main-\(root)", "9000 9000 520 640 0 0 1024 768"])
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

    func testSettingsShowStorageWithoutAStoreYet() {
        let app = launched(root: UUID().uuidString)
        openSettings(app)
        let messages = app.staticTexts["storage-messages"]
        XCTAssertTrue(messages.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.staticTexts["storage-identity"].exists, app.debugDescription)
        XCTAssertTrue(app.staticTexts["status-traffic"].exists, app.debugDescription)
        XCTAssertTrue(app.staticTexts["status-messages"].exists, app.debugDescription)
        // No client store before the network is set up: its path would promise a file.
        XCTAssertFalse(app.staticTexts["storage-messages-path"].exists, app.debugDescription)
        app.terminate()
    }

    func testTurningOffTheIconTurnsOffCloseToMenuBar() {
        let app = launched(root: UUID().uuidString)
        openSettings(app)
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
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = defaults + ["--ui-test-slot-root=\(root)"]
        app.launch()
        return app
    }

    private func openSettings(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        create.click()
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10), app.debugDescription)
        settings.click()
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
