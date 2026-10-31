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
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
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

    func testSettingsListsTheSelectedIdentity() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        let peerID = shownPeerId(app)
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10))
        settings.tap()
        let selected = app.descendants(matching: .any)["settings-active"]
        XCTAssertTrue(selected.waitForExistence(timeout: 10))
        XCTAssertTrue(selected.label.contains(peerID), selected.label)
        XCTAssertTrue(app.buttons["settings-network"].exists)
        app.terminate()
    }
}

extension LaunchTests {
    @objc
    func testSettingsRemoveCancelThenSelectSurvivorAfterConfirm() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        let first = shownPeerId(app)
        app.buttons["settings-open"].tap()
        createAnotherIdentity(app)
        let peer = app.staticTexts["peer-id"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label != %@", first), object: peer
        ),
        ], timeout: 10), .completed)
        let second = shownPeerId(app)
        app.buttons["settings-open"].tap()
        let remove = app.buttons["settings-remove"]
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        remove.tap()
        let alert = app.alerts["Remove identity?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 10))
        XCTAssertTrue(alert.staticTexts.matching(NSPredicate(
            format: "label CONTAINS %@", second
        )).firstMatch.exists)
        alert.buttons["Keep identity"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings-active"].exists)
        remove.tap()
        alert.buttons["Remove permanently"].tap()
        XCTAssertTrue(app.staticTexts["identity-picker"].waitForExistence(timeout: 10))
        let select = app.buttons.matching(NSPredicate(
            format: "identifier BEGINSWITH %@", "identity-picker-select-"
        )).firstMatch
        XCTAssertTrue(select.waitForExistence(timeout: 10))
        select.tap()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
    }
}

extension LaunchTests {
    @objc
    func testRemovalConfirmationRussianAndPseudoLanguage() {
        let russian = unlockedApp()
        russian.launchArguments += ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"]
        russian.launch()
        createIdentity(russian)
        russian.buttons["settings-open"].tap()
        let remove = russian.buttons["settings-remove"]
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        XCTAssertEqual(remove.label, "\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{44d}\u{442}\u{443} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}")
        remove.tap()
        let alert = russian.alerts["\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 10))
        XCTAssertTrue(alert.buttons["\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{43d}\u{430}\u{432}\u{441}\u{435}\u{433}\u{434}\u{430}"].exists)
        alert.buttons["\u{41e}\u{441}\u{442}\u{430}\u{432}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}"].tap()
        XCTAssertTrue(russian.descendants(matching: .any)["settings-active"].exists)
        russian.terminate()

        let pseudo = unlockedApp()
        pseudo.launchArguments += ["-NSDoubleLocalizedStrings", "YES"]
        pseudo.launch()
        createIdentity(pseudo)
        pseudo.buttons["settings-open"].tap()
        let pseudoRemove = pseudo.buttons["settings-remove"]
        XCTAssertTrue(pseudoRemove.waitForExistence(timeout: 10))
        XCTAssertTrue(pseudoRemove.label.hasPrefix("Remove this identity"))
        XCTAssertTrue(pseudoRemove.label.count >= 2 * "Remove this identity".count)
        XCTAssertTrue(pseudoRemove.isHittable)
        pseudoRemove.tap()
        let cancel = pseudo.alerts.buttons.matching(NSPredicate(
            format: "label BEGINSWITH %@", "Keep identity"
        )).firstMatch
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        XCTAssertTrue(cancel.isHittable)
        cancel.tap()
        pseudo.terminate()
    }
}

extension LaunchTests {
    @objc
    func testUnreadableSelectedKeyShowsRussianRecoveryWithoutReplacement() {
        let app = unlockedApp()
        app.launchArguments += [
            "--ui-test-corrupt-identity-key", "-AppleLanguages", "(ru)",
            "-AppleLocale", "ru_RU",
        ]
        app.launch()
        createIdentity(app)
        app.buttons["settings-open"].tap()
        let selected = app.descendants(matching: .any)["settings-active"]
        XCTAssertTrue(selected.waitForExistence(timeout: 10))
        XCTAssertTrue(selected.label.contains("\u{41a}\u{43b}\u{44e}\u{447} "
            + "\u{43d}\u{435}\u{434}\u{43e}\u{441}\u{442}\u{443}\u{43f}\u{435}\u{43d}"))
        XCTAssertFalse(app.buttons["settings-remove"].exists)
        app.terminate()

        app.launch()
        let recovery = app.staticTexts["identity-recovery"]
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        XCTAssertTrue(recovery.label.hasPrefix("\u{41d}\u{435} "
            + "\u{443}\u{434}\u{430}\u{43b}\u{43e}\u{441}\u{44c} "
            + "\u{43e}\u{442}\u{43a}\u{440}\u{44b}\u{442}\u{44c}"))
        XCTAssertFalse(app.buttons["identity-create"].exists)
        app.buttons["identity-retry"].tap()
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        app.terminate()
    }
}

extension LaunchTests {
    func testSettingsCreatesAndSwitchesWithoutReplacingKeys() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        let first = shownPeerId(app)
        app.buttons["settings-open"].tap()
        createAnotherIdentity(app)
        let peer = app.staticTexts["peer-id"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label != %@", first), object: peer
        ),
        ], timeout: 10), .completed)
        let second = shownPeerId(app)
        XCTAssertNotEqual(second, first)
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
        app.buttons["settings-open"].tap()
        let select = app.buttons.matching(NSPredicate(
            format: "identifier BEGINSWITH %@", "settings-select-"
        )).firstMatch
        XCTAssertTrue(select.waitForExistence(timeout: 10))
        select.tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", first), object: peer
        ),
        ], timeout: 10), .completed)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
    }

    func testNetworkDraftSurvivesRelaunch() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let rows = app.textViews["network-bootstrap"]
        let name = app.textFields["network-name"]
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        rows.tap()
        typeSlowly("one.example:33445\ntwo.example:7070", into: rows)
        reveal(name, in: app).tap()
        name.typeText("friends")
        reveal(save, in: app).tap()
        let status = reveal(app.staticTexts["network-draft-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "Draft saved"), object: status
        ),
        ], timeout: 10), .completed)
        XCTAssertTrue(status.label.contains("Draft saved"), status.label)

        app.terminate()
        app.launch()
        openNetwork(app)
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        XCTAssertEqual(app.textViews["network-bootstrap"].value as? String,
                       "one.example:33445\ntwo.example:7070")
        XCTAssertEqual(reveal(app.textFields["network-name"], in: app).value as? String, "friends")
        app.terminate()
    }

    func testDamagedNetworkDraftShowsReadError() {
        let app = unlockedApp()
        app.launchArguments += ["--ui-test-corrupt-network-draft"]
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let status = reveal(app.staticTexts["network-draft-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "could not be read"), object: status
        ),
        ], timeout: 10), .completed)
        XCTAssertTrue(status.label.contains("could not be read"), status.label)
        XCTAssertFalse(app.buttons["network-save"].isEnabled)
        app.terminate()
    }

    func testNetworkCheckShowsInvalidRow() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("not-an-address", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        // The rows below the check exist once it ends. A drag after the keyboard closes sends
        // the form back to the top, out of the lazy rows, so read them without reveal.
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(result.waitForExistence(timeout: 10))
        XCTAssertTrue(result.label.contains("Invalid address"), result.label)
        XCTAssertTrue(app.buttons["network-continue"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["network-continue"].isEnabled)
        let retry = app.buttons["network-check"]
        XCTAssertEqual(retry.label, "Retry connection")
        retry.tap()
        XCTAssertTrue(result.waitForExistence(timeout: 10))
        app.terminate()
    }

    func testLocalNodeConnectsOnlyAfterContinue() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        let peerID = shownPeerId(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        let next = reveal(app.buttons["network-continue"], in: app)
        XCTAssertFalse(next.isEnabled)
        reveal(app.buttons["network-check"], in: app).tap()
        let status = reveal(app.staticTexts["network-check-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "compatible node answered"),
            object: status
        ),
        ], timeout: 30), .completed)
        XCTAssertTrue(reveal(next, in: app).isEnabled)
        next.tap()
        XCTAssertTrue(app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10))
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), peerID)
        XCTAssertTrue(app.staticTexts["app-ready"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["network-edit"].exists)
        XCTAssertFalse(app.buttons["network-save"].exists)
        app.terminate()
    }

    func testNetworkFailureRowsKeepTheirOrder() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("not-an-address\n127.0.0.1:33446\n127.0.0.1:33446\n127.0.0.1:33445/bad", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        let expected = ["Invalid address", "Unreachable", "Duplicate address", "Invalid peer ID"]
        for (index, word) in expected.enumerated() {
            let result = app.descendants(matching: .any)["network-row-\(index)"]
            XCTAssertTrue(reveal(result, in: app, timeout: 15).exists)
            XCTAssertTrue(result.label.contains(word), result.label)
        }
        XCTAssertFalse(reveal(app.buttons["network-continue"], in: app).isEnabled)
        app.terminate()
    }

    func testWrongNetworkDisablesContinue() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        let name = app.textFields["network-name"]
        reveal(name, in: app).tap()
        name.typeText("city")
        reveal(app.buttons["network-check"], in: app).tap()
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(reveal(result, in: app, timeout: 15).exists)
        XCTAssertTrue(result.label.contains("Wrong network"), result.label)
        XCTAssertFalse(reveal(app.buttons["network-continue"], in: app).isEnabled)
        app.terminate()
    }

    func testCancelRetryAndRestartKeepDraft() {
        let app = unlockedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: save
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33447", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        let cancel = reveal(app.buttons["network-cancel"], in: app)
        XCTAssertTrue(cancel.exists)
        cancel.tap()
        let status = reveal(app.staticTexts["network-check-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "Check cancelled"), object: status
        ),
        ], timeout: 15), .completed)
        let retry = app.buttons["network-check"]
        XCTAssertEqual(retry.label, "Retry connection")
        retry.tap()
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(reveal(result, in: app, timeout: 15).exists)
        XCTAssertTrue(result.label.contains("Unreachable"), result.label)
        XCTAssertFalse(reveal(app.buttons["network-continue"], in: app).isEnabled)

        app.terminate()
        app.launch()
        openNetwork(app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        XCTAssertEqual(app.textViews["network-bootstrap"].value as? String, "127.0.0.1:33447")
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
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
    }

    /// The button stays disabled until the identity list loads, and `tap()` does not wait for that.
    private func createAnotherIdentity(_ app: XCUIApplication) {
        let create = app.buttons["settings-create"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "isEnabled == true"), object: create
        ),
        ], timeout: 10), .completed, app.debugDescription)
        create.tap()
    }

    private func shownPeerId(_ app: XCUIApplication) -> String {
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10))
        return peerId.label
    }

    private func openNetwork(_ app: XCUIApplication) {
        if app.buttons["network-save"].waitForExistence(timeout: 10) { return }
        let link = app.buttons["network-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
    }

    /// Practice D1: Xcode's double-length pseudo-language; every string comes from the catalog.
    func testDoubleLengthPseudoLanguage() {
        let app = unlockedApp()
        app.launchArguments += ["-NSDoubleLocalizedStrings", "YES"]
        app.launch()
        createIdentity(app)
        // Doubled header rows push the lazy form's save button below the screen.
        let save = reveal(app.buttons["network-save"], in: app)
        XCTAssertTrue(save.exists)
        let text = save.label
        XCTAssertTrue(text.hasPrefix("Save draft") && text.count >= 2 * "Save draft".count, text)
        XCTAssertTrue(save.isHittable)
        app.terminate()
    }

    func testRussianLanguage() {
        let app = unlockedApp()
        app.launchArguments += ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"]
        app.launch()
        createIdentity(app)
        let save = app.buttons["network-save"]
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        XCTAssertEqual(save.label, "\u{0421}\u{043e}\u{0445}\u{0440}\u{0430}\u{043d}\u{0438}\u{0442}\u{044c} "
                       + "\u{0447}\u{0435}\u{0440}\u{043d}\u{043e}\u{0432}\u{0438}\u{043a}")
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
