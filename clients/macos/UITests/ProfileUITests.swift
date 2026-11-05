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
        revealEditable(month, in: app)
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

    /// Hiding needs a publication, stays across a relaunch, and only a new publication undoes it.
    func testHideThenPublishShowsAgain() {
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let hide = app.buttons["profile-hide"]
        reveal(hide, in: app)
        XCTAssertFalse(hide.isEnabled)
        let month = app.textFields["profile-birth-month"]
        revealEditable(month, in: app)
        month.click()
        month.typeText("5")
        let save = app.buttons["profile-save"]
        reveal(save, in: app)
        save.click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.checkBoxes["profile-confirm-adult"]
        ),
        ], timeout: 10), .completed)
        publish(app)
        let readback = app.descendants(matching: .any)["profile-published-readback"]
        XCTAssertTrue(readback.waitForExistence(timeout: 60), app.debugDescription)
        reveal(hide, in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: hide
        ),
        ], timeout: 10), .completed)
        hide.click()
        let notice = app.staticTexts["profile-hidden-notice"]
        XCTAssertTrue(notice.waitForExistence(timeout: 60), app.debugDescription)
        XCTAssertFalse(readback.exists)
        app.terminate()

        app.launch()
        openProfile(app)
        XCTAssertTrue(notice.waitForExistence(timeout: 30), app.debugDescription)
        publish(app)
        XCTAssertTrue(readback.waitForExistence(timeout: 60), app.debugDescription)
        XCTAssertTrue(hide.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(notice.exists)
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
            XCTAssertTrue(app.descendants(matching: .any)[row].waitForExistence(timeout: 10), "\(row)\n\(app.debugDescription)")
        }
        app.buttons["view-back"].click()
        XCTAssertTrue(about.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(about.value as? String, "Plain text", app.debugDescription)
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

    /// Photos come through the debug seam, as a test cannot drive the file dialog. Their order is
    /// the core's to test; here the controls follow it and what is left survives a restart.
    func testPhotosAddMoveRemoveSurviveRestart() {
        let app = isolatedApp()
        app.launchArguments += ["--ui-test-pick-photo"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let add = app.buttons["profile-photo-add"]
        for index in 0..<2 {
            waitEnabled(add)
            add.click()
            XCTAssertTrue(app.descendants(matching: .any)["profile-photo-\(index)"].waitForExistence(timeout: 30),
                          app.debugDescription)
        }
        let right = app.buttons["profile-photo-right-0"]
        waitEnabled(right)
        right.click()
        XCTAssertFalse(app.buttons["profile-photo-left-0"].isEnabled)
        XCTAssertFalse(app.buttons["profile-photo-right-1"].isEnabled)
        let remove = app.buttons["profile-photo-remove-1"]
        waitEnabled(remove)
        remove.click()
        let confirm = app.sheets.buttons["Remove"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10), app.debugDescription)
        confirm.click()
        let second = app.descendants(matching: .any)["profile-photo-1"]
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: second)
        XCTAssertEqual(XCTWaiter().wait(for: [gone], timeout: 30), .completed, app.debugDescription)
        app.terminate()
        app.launch()
        openProfile(app)
        XCTAssertTrue(app.descendants(matching: .any)["profile-photo-0"].waitForExistence(timeout: 30))
        XCTAssertFalse(second.exists)
        app.terminate()
    }

    private func waitEnabled(_ element: XCUIElement) {
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: element)
        XCTAssertEqual(XCTWaiter().wait(for: [enabled], timeout: 30), .completed, element.debugDescription)
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

extension ProfileUITests {
    /// Removing a published identity says it withdraws; without a node it offers Retry, Keep and
    /// Remove anyway, and with one the removal goes through.
    @objc
    func testRemovalWithdrawsAndOffersRemoveAnywayWithoutANode() {
        let app = isolatedApp()
        let base = app.launchArguments
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        let month = app.textFields["profile-birth-month"]
        revealEditable(month, in: app)
        month.click()
        month.typeText("5")
        let save = app.buttons["profile-save"]
        reveal(save, in: app)
        save.click()
        waitEnabled(app.checkBoxes["profile-confirm-adult"])
        publish(app)
        let readback = app.descendants(matching: .any)["profile-published-readback"]
        XCTAssertTrue(readback.waitForExistence(timeout: 60), app.debugDescription)
        app.terminate()

        app.launchArguments = base + ["--ui-test-fail-withdraw"]
        app.launch()
        let sheet = openRemoval(app, says: "withdrawn from search first")
        sheet.buttons["Remove permanently"].click()
        XCTAssertTrue(sheet.buttons["Remove anyway"].waitForExistence(timeout: 30), app.debugDescription)
        sheetSays(sheet, "within 30 days")
        sheet.buttons["Retry"].click()
        XCTAssertTrue(sheet.buttons["Remove anyway"].waitForExistence(timeout: 30), app.debugDescription)
        sheet.buttons["Keep identity"].click()
        XCTAssertTrue(app.descendants(matching: .any)["settings-active"].waitForExistence(timeout: 10))
        app.terminate()

        app.launchArguments = base
        app.launch()
        openRemoval(app, says: "withdrawn from search first").buttons["Remove permanently"].click()
        XCTAssertTrue(app.buttons["identity-create"].waitForExistence(timeout: 60), app.debugDescription)
        app.terminate()
    }

    private func openRemoval(_ app: XCUIApplication, says text: String) -> XCUIElement {
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 30), app.debugDescription)
        settings.click()
        let remove = app.buttons["settings-remove"]
        reveal(remove, in: app)
        waitEnabled(remove)
        remove.click()
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10), app.debugDescription)
        sheetSays(sheet, text)
        return sheet
    }

    private func sheetSays(_ sheet: XCUIElement, _ text: String) {
        XCTAssertTrue(sheet.staticTexts.matching(NSPredicate(
            format: "label CONTAINS %@ OR value CONTAINS %@", text, text
        )).firstMatch.exists, sheet.debugDescription)
    }
}
