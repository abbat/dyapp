import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class ProfileUITests: XCTestCase {
    /// An optional birth part survives restart; failed publishes show localized errors and ask for
    /// new confirmations; a later launch reuses the proof and shows the signed loopback readback.
    func testProfileDraftRestartFailedPublishAndSignedReadback() {
        let app = isolatedApp()
        let base = app.launchArguments
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let month = app.textFields["profile-birth-month"]
        reveal(month, in: app)
        month.click()
        month.typeText("5")
        let save = app.buttons["profile-save"]
        reveal(save, in: app)
        save.click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.checkBoxes["profile-confirm-adult"]
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
            app.launchArguments = language + base + ["--ui-test-fail-publish"]
            assertFailedPublish(app, month: month, expected: expected)
        }

        app.launchArguments = base
        app.launch()
        openProfile(app)
        publish(app)
        let readback = app.descendants(matching: .any).matching(identifier: "profile-published-readback")
        XCTAssertTrue(readback.firstMatch.waitForExistence(timeout: 60), app.debugDescription)
        // One element per text, or one combined element whose label joins them with ", ".
        let texts = readback.allElementsBoundByIndex.map { $0.value as? String ?? $0.label }
            .joined(separator: ", ").components(separatedBy: ", ")
        for text in ["Published profile verified", "1", "5"] {
            XCTAssertTrue(texts.contains(text), "\(texts)")
        }
        XCTAssertFalse(texts.contains("Birth year (optional)"), "\(texts)")
        app.terminate()
    }

    /// A height that is no number is refused with its own message and saves nothing; the typed
    /// fields then survive a restart and reach the preview.
    func testTypedFieldsRefuseBadHeightAndSurviveRestart() {
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let height = app.textFields["profile-height"]
        reveal(height, in: app)
        height.click()
        height.typeText("tall")
        let save = app.buttons["profile-save"]
        reveal(save, in: app)
        save.click()
        let status = app.staticTexts["profile-status"]
        XCTAssertTrue(status.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue((status.value as? String ?? status.label).contains("272"), app.debugDescription)

        reveal(height, in: app, deltaY: 300)
        height.click()
        height.typeKey("a", modifierFlags: .command)
        height.typeText("170")
        fillAboutAndSeeking(app)
        reveal(save, in: app)
        save.click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == 'Draft saved' OR label == 'Draft saved'"), object: status
        ),
        ], timeout: 10), .completed, app.debugDescription)
        app.terminate()

        app.launch()
        openProfile(app)
        reveal(height, in: app)
        XCTAssertEqual(height.value as? String, "170")
        let about = app.textViews["profile-about"]
        reveal(about, in: app)
        XCTAssertEqual(about.value as? String, "Plain text")
        let gender = app.checkBoxes["profile-seeking-gender-1"]
        reveal(gender, in: app)
        XCTAssertEqual(gender.value as? Int, 1)
        let ageFrom = app.textFields["profile-seeking-age-from"]
        reveal(ageFrom, in: app)
        XCTAssertEqual(ageFrom.value as? String, "30")
        let preview = app.buttons["profile-preview"]
        reveal(preview, in: app)
        preview.click()
        for row in ["preview-height", "preview-about", "preview-seeking-genders", "preview-seeking-age-from"] {
            XCTAssertTrue(app.descendants(matching: .any)[row].waitForExistence(timeout: 10), row)
        }
        app.terminate()
    }

    private func fillAboutAndSeeking(_ app: XCUIApplication) {
        let about = app.textViews["profile-about"]
        reveal(about, in: app)
        about.click()
        about.typeText("Plain text")
        let gender = app.checkBoxes["profile-seeking-gender-1"]
        reveal(gender, in: app)
        gender.click()
        let ageFrom = app.textFields["profile-seeking-age-from"]
        reveal(ageFrom, in: app)
        ageFrom.click()
        ageFrom.typeText("30")
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
        let text = error.value as? String ?? error.label
        XCTAssertTrue(expected(text), text)
        XCTAssertFalse(app.buttons["profile-prepare-publish"].isEnabled)
        XCTAssertEqual(app.checkBoxes["profile-confirm-adult"].value as? Int, 0)
        app.terminate()
    }

    private func connectLocalNode(_ app: XCUIApplication) {
        openNetwork(app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.click()
        rows.typeText("127.0.0.1:33445")
        app.buttons["network-check"].click()
        let next = app.buttons["network-continue"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: next
        ),
        ], timeout: 15), .completed)
        next.click()
    }

    private func openProfile(_ app: XCUIApplication) {
        // The form creates rows lazily: the save button is off screen, the first field is not.
        if app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 5) { return }
        let link = app.buttons["profile-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.click()
        XCTAssertTrue(app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10))
    }

    private func isolatedApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)"]
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
