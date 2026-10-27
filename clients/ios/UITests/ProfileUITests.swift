import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class ProfileUITests: XCTestCase {
    /// An optional birth part survives restart; failed publishes show localized errors and ask for
    /// new confirmations; a later launch reuses the proof and shows the signed loopback readback.
    func testProfileDraftRestartFailedPublishAndSignedReadback() {
        let app = unlockedApp()
        let base = app.launchArguments
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let month = app.textFields["profile-birth-month"]
        reveal(month, in: app)
        month.tap()
        month.typeText("5")
        let save = app.buttons["profile-save"]
        reveal(save, in: app)
        save.tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.switches["profile-confirm-adult"]
        ),
        ], timeout: 10), .completed)
        app.terminate()

        let failures: [([String], (String) -> Bool)] = [
            (["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"], {
                $0.hasPrefix("\u{410}\u{43a}\u{442}\u{438}\u{432}\u{43d}\u{430}\u{44f} \u{441}\u{435}\u{442}\u{44c}")
            }),
            (["-NSDoubleLocalizedStrings", "YES"], {
                $0.hasPrefix("The active network is unavailable") && $0.count >= 2 * 66
            }),
        ]
        for (language, expected) in failures {
            app.launchArguments = base + language + ["--ui-test-fail-publish"]
            assertFailedPublish(app, month: month, expected: expected)
        }

        app.launchArguments = base
        app.launch()
        openProfile(app)
        publish(app)
        let readback = app.descendants(matching: .any).matching(identifier: "profile-published-readback")
        XCTAssertTrue(readback.firstMatch.waitForExistence(timeout: 60), app.debugDescription)
        // One element per text, or one combined element whose label joins them with ", ".
        let texts = readback.allElementsBoundByIndex.map(\.label).joined(separator: ", ")
            .components(separatedBy: ", ")
        for text in ["Published profile verified", "1", "5"] {
            XCTAssertTrue(texts.contains(text), "\(texts)")
        }
        XCTAssertFalse(texts.contains("Birth year (optional)"), "\(texts)")
        app.terminate()
    }

    /// Relaunch, keep the saved month, and see a localized failure that clears the confirmations.
    private func assertFailedPublish(_ app: XCUIApplication, month: XCUIElement,
                                     expected: (String) -> Bool) {
        app.launch()
        openProfile(app)
        reveal(month, in: app)
        XCTAssertEqual(month.value as? String, "5")
        publish(app)
        let error = app.staticTexts["profile-publish-error"]
        XCTAssertTrue(error.waitForExistence(timeout: 60), app.debugDescription)
        XCTAssertTrue(expected(error.label), error.label)
        XCTAssertFalse(app.buttons["profile-prepare-publish"].isEnabled)
        XCTAssertEqual(app.switches["profile-confirm-adult"].value as? String, "0")
        app.terminate()
    }

    private func connectLocalNode(_ app: XCUIApplication) {
        openNetwork(app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        let next = reveal(app.buttons["network-continue"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: next
        ),
        ], timeout: 15), .completed)
        // The check rows push Continue down after the first reveal.
        reveal(next, in: app).tap()
    }

    private func openProfile(_ app: XCUIApplication) {
        if app.buttons["profile-save"].waitForExistence(timeout: 5) { return }
        let link = app.buttons["profile-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["profile-save"].waitForExistence(timeout: 10))
    }

    /// Give both fresh confirmations, then start the proof and publish.
    private func publish(_ app: XCUIApplication) {
        for id in ["profile-confirm-adult", "profile-confirm-public"] {
            let toggle = app.switches[id]
            reveal(toggle, in: app)
            // The row's centre is its label; the switch sits at the trailing edge.
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
            XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "value == '1'"), object: toggle
            ),
            ], timeout: 10), .completed)
        }
        let button = app.buttons["profile-prepare-publish"]
        reveal(button, in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: button
        ),
        ], timeout: 10), .completed)
        button.tap()
    }

    // Every launch starts with the app lock skipped, so the start screen renders.
    private func unlockedApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        return app
    }

    private func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
    }

    private func openNetwork(_ app: XCUIApplication) {
        if app.buttons["network-save"].waitForExistence(timeout: 10) { return }
        let link = app.buttons["network-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
    }
}
