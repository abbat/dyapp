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
        XCTAssertTrue(reveal(readback.firstMatch, in: app, timeout: 60).exists, app.debugDescription)
        // One element per text, or one combined element whose label joins them with ", ".
        let texts = readback.allElementsBoundByIndex.map(\.label).joined(separator: ", ")
            .components(separatedBy: ", ")
        for text in ["Published profile verified", "1", "5"] {
            XCTAssertTrue(texts.contains(text), "\(texts)")
        }
        XCTAssertFalse(texts.contains("Birth year (optional)"), "\(texts)")
        app.terminate()
    }

    /// Only a suggested place saves; the preview names the country and the picked place.
    func testPlacePickerSavesOnlyASuggestedPlace() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openProfile(app)
        reveal(app.buttons["profile-country"], in: app).tap()
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 10), app.debugDescription)
        search.tap()
        search.typeText("Germany")
        app.buttons["country-DE"].tap()

        let place = app.textFields["profile-place"]
        reveal(place, in: app).tap()
        typeSlowly("Nowhere", into: place)
        reveal(app.buttons["profile-save"], in: app).tap()
        let status = app.staticTexts["profile-status"]
        XCTAssertTrue(reveal(status, in: app).waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(status.label, "Pick a place from the suggestions for the chosen country.")

        // "Berl" suggests Berlin, GeoNames 2950159; picking it saves its ID.
        reveal(place, in: app).tap()
        place.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: "Nowhere".count))
        typeSlowly("Berl", into: place)
        reveal(app.buttons["place-2950159"], in: app).tap()
        reveal(app.buttons["profile-save"], in: app).tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == 'Draft saved'"), object: status
        ),
        ], timeout: 10), .completed)
        reveal(app.buttons["profile-preview"], in: app).tap()
        let row = app.descendants(matching: .any)["preview-place"]
        XCTAssertTrue(row.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(row.label.contains("Berlin"), row.label)
        XCTAssertTrue(app.descendants(matching: .any)["preview-country"].exists)
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
        // The error row sits below the proof progress, past the lazy form's first screen.
        XCTAssertTrue(reveal(error, in: app, timeout: 60).exists, app.debugDescription)
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
        // Reveal only after the check: its rows move Continue while they appear, and a hit test
        // of a moving button fails the test. A slow simulator takes about 20 s to check.
        let next = app.buttons["network-continue"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: next
        ),
        ], timeout: 30), .completed)
        reveal(next, in: app).tap()
    }

    private func openProfile(_ app: XCUIApplication) {
        // The form creates rows lazily: the save button is off screen, the first field is not.
        if app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 5) { return }
        let link = app.buttons["profile-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10))
    }

    /// Give both fresh confirmations, then start the proof and publish.
    private func publish(_ app: XCUIApplication) {
        for id in ["profile-confirm-adult", "profile-confirm-public"] {
            let toggle = app.switches[id]
            reveal(toggle, in: app)
            XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "enabled == true"), object: toggle
            ),
            ], timeout: 10), .completed)
            // The row's centre is its label; newer iOS nests the tappable switch inside the row,
            // older iOS has none and the switch sits at the trailing edge.
            let knob = toggle.switches.firstMatch
            if knob.exists {
                knob.tap()
            } else {
                toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
            }
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
