import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class LanguageTests: XCTestCase {
    /// Practice D1: Xcode's double-length pseudo-language; every string comes from the catalog.
    func testDoubleLengthPseudoLanguage() {
        let app = launchedWithIdentity(defaults: ["-NSDoubleLocalizedStrings", "YES"])
        let save = app.buttons["network-save"]
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        let text = save.label
        XCTAssertTrue(text.hasPrefix("Save draft") && text.count >= 2 * "Save draft".count, text)
        // Doubled header rows can push the save button below the window; scroll the form down.
        for _ in 0..<10 where !save.isHittable {
            app.scrollViews.firstMatch.scroll(byDeltaX: 0, deltaY: -200)
        }
        XCTAssertTrue(save.isHittable)
        app.terminate()
    }

    func testRussianLanguage() {
        let app = launchedWithIdentity(defaults: [
            "-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU",
        ])
        let save = app.buttons["network-save"]
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        XCTAssertEqual(
            save.label,
            "\u{0421}\u{043e}\u{0445}\u{0440}\u{0430}\u{043d}\u{0438}\u{0442}\u{044c} "
                + "\u{0447}\u{0435}\u{0440}\u{043d}\u{043e}\u{0432}\u{0438}\u{043a}")
        app.terminate()
    }

    /// Put `-Key value` defaults before the slot root; see `LaunchTests.isolatedApp`.
    private func launchedWithIdentity(defaults: [String]) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = defaults + ["--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
        XCTAssertTrue(
            app.buttons["settings-open"].waitForExistence(timeout: 10), app.debugDescription)
        return app
    }
}
