import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class LaunchTests: XCTestCase {
    func testLaunchRenderAndTerminate() {
        let app = unlockedApp()
        app.launch()
        XCTAssertTrue(app.buttons["identity-create"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["peer-id"].exists)
        createIdentity(app)
        XCTAssertTrue(app.staticTexts["DYApp"].waitForExistence(timeout: 10))
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertEqual(ready.label, "Ready")
        XCTAssertTrue(ready.isHittable)
        // The peer ID comes from the Rust core through the UniFFI bindings.
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10))
        XCTAssertNotNil(peerId.label.range(of: "^[0-9a-f]{64}$", options: .regularExpression), "Unexpected peer ID")
        app.terminate()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    /// The stored identity key survives a restart: the second launch unwraps the same peer ID.
    func testRelaunchKeepsPeerId() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        let first = shownPeerId(app)
        XCTAssertNotNil(first.range(of: "^[0-9a-f]{64}$", options: .regularExpression), first)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
    }

    // Every launch starts with the app lock skipped, so the start screen renders.
    private func unlockedApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        return app
    }

    private func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10))
        create.tap()
    }

    private func shownPeerId(_ app: XCUIApplication) -> String {
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10))
        return peerId.label
    }

    /// Practice D1: Xcode's double-length pseudo-language; every string comes from the catalog.
    func testDoubleLengthPseudoLanguage() {
        let app = unlockedApp()
        app.launchArguments += ["-NSDoubleLocalizedStrings", "YES"]
        app.launch()
        createIdentity(app)
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        let text = ready.label
        XCTAssertTrue(text.hasPrefix("Ready") && text.count >= 2 * "Ready".count, text)
        XCTAssertTrue(ready.isHittable)
        app.terminate()
    }

    func testRussianLanguage() {
        let app = unlockedApp()
        app.launchArguments += ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"]
        app.launch()
        createIdentity(app)
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertEqual(ready.label, "\u{0413}\u{043e}\u{0442}\u{043e}\u{0432}\u{043e}")
        app.terminate()
    }

    func testHiddenReadyDoesNotSatisfyLaunchContract() {
        let app = unlockedApp()
        app.launchArguments += ["--break-ui"]
        app.launch()
        createIdentity(app)
        XCTAssertTrue(app.staticTexts["DYApp"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
        app.terminate()
    }
}
