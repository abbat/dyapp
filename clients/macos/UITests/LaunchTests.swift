import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class LaunchTests: XCTestCase {
    func testLaunchRenderAndTerminate() {
        let app = isolatedApp()
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
        XCTAssertNotNil((peerId.value as? String ?? "").range(of: "^[0-9a-f]{64}$", options: .regularExpression), "Unexpected peer ID")
        app.terminate()
        XCTAssertTrue(app.wait(for: .notRunning, timeout: 10))
    }

    /// The stored identity key survives a restart: the second launch unwraps the same peer ID.
    func testRelaunchKeepsPeerId() {
        let app = isolatedApp()
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
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        let peerID = shownPeerId(app)
        let settings = app.buttons["settings-open"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10))
        settings.click()
        let selected = app.descendants(matching: .any)["settings-active"]
        XCTAssertTrue(selected.waitForExistence(timeout: 10))
        let text = selected.value as? String ?? selected.label
        XCTAssertTrue(text.contains(peerID), text)
        XCTAssertTrue(app.buttons["settings-network"].exists)
        app.terminate()
    }
}

extension LaunchTests {
    @objc
    func testSettingsRemoveCancelThenSelectSurvivorAfterConfirm() {
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        let first = shownPeerId(app)
        app.buttons["settings-open"].tap()
        app.buttons["settings-create"].tap()
        let peer = app.staticTexts["peer-id"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value != %@", first), object: peer
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
        let russian = isolatedApp()
        russian.launchArguments.insert(contentsOf: ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"], at: 0)
        russian.launch()
        createIdentity(russian)
        russian.buttons["settings-open"].tap()
        let remove = russian.buttons["settings-remove"]
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        XCTAssertEqual(remove.value as? String ?? remove.label, "\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{44d}\u{442}\u{443} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}")
        remove.tap()
        let alert = russian.alerts["\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 10))
        XCTAssertTrue(alert.buttons["\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{43d}\u{430}\u{432}\u{441}\u{435}\u{433}\u{434}\u{430}"].exists)
        alert.buttons["\u{41e}\u{441}\u{442}\u{430}\u{432}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}"].tap()
        XCTAssertTrue(russian.descendants(matching: .any)["settings-active"].exists)
        russian.terminate()

        let pseudo = isolatedApp()
        pseudo.launchArguments.insert(contentsOf: ["-NSDoubleLocalizedStrings", "YES"], at: 0)
        pseudo.launch()
        createIdentity(pseudo)
        pseudo.buttons["settings-open"].tap()
        let pseudoRemove = pseudo.buttons["settings-remove"]
        XCTAssertTrue(pseudoRemove.waitForExistence(timeout: 10))
        let title = pseudoRemove.value as? String ?? pseudoRemove.label
        XCTAssertTrue(title.hasPrefix("Remove this identity"))
        XCTAssertTrue(title.count >= 2 * "Remove this identity".count)
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
        let app = isolatedApp()
        app.launchArguments += [
            "--ui-test-corrupt-identity-key", "-AppleLanguages", "(ru)",
            "-AppleLocale", "ru_RU",
        ]
        app.launch()
        createIdentity(app)
        app.buttons["settings-open"].tap()
        let selected = app.descendants(matching: .any)["settings-active"]
        XCTAssertTrue(selected.waitForExistence(timeout: 10))
        let selectedText = selected.value as? String ?? selected.label
        XCTAssertTrue(selectedText.contains("\u{41a}\u{43b}\u{44e}\u{447} "
            + "\u{43d}\u{435}\u{434}\u{43e}\u{441}\u{442}\u{443}\u{43f}\u{435}\u{43d}"))
        XCTAssertFalse(app.buttons["settings-remove"].exists)
        app.terminate()

        app.launch()
        let recovery = app.staticTexts["identity-recovery"]
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        let recoveryText = recovery.value as? String ?? recovery.label
        XCTAssertTrue(recoveryText.hasPrefix("\u{41d}\u{435} "
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
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        let first = shownPeerId(app)
        app.buttons["settings-open"].tap()
        app.buttons["settings-create"].tap()
        let peer = app.staticTexts["peer-id"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value != %@", first), object: peer
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
            predicate: NSPredicate(format: "value == %@", first), object: peer
        ),
        ], timeout: 10), .completed)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
    }

    func testNetworkDraftSurvivesRelaunch() {
        let app = isolatedApp()
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
        rows.typeText("one.example:33445\ntwo.example:7070")
        name.tap()
        name.typeText("friends")
        save.tap()
        let status = app.staticTexts["network-draft-status"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "Draft saved"), object: status
        ),
        ], timeout: 10), .completed)
        XCTAssertTrue((status.value as? String ?? "").contains("Draft saved"))

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
        XCTAssertEqual(app.textFields["network-name"].value as? String, "friends")
        app.terminate()
    }

    func testDamagedNetworkDraftShowsReadError() {
        let app = isolatedApp()
        app.launchArguments += ["--ui-test-corrupt-network-draft"]
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let status = app.staticTexts["network-draft-status"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "could not be read"), object: status
        ),
        ], timeout: 10), .completed)
        XCTAssertTrue((status.value as? String ?? "").contains("could not be read"))
        XCTAssertFalse(app.buttons["network-save"].isEnabled)
        app.terminate()
    }

    func testNetworkCheckShowsInvalidRow() {
        let app = isolatedApp()
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
        rows.typeText("not-an-address")
        app.buttons["network-check"].tap()
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(result.waitForExistence(timeout: 10))
        let displayed = [result.label, result.value as? String ?? ""].joined(separator: " ")
        XCTAssertTrue(displayed.contains("Invalid address"), displayed)
        XCTAssertFalse(app.buttons["network-continue"].isEnabled)
        let retry = app.buttons["network-check"]
        XCTAssertEqual(retry.label, "Retry connection")
        retry.tap()
        XCTAssertTrue(result.waitForExistence(timeout: 10))
        app.terminate()
    }

    func testLocalNodeConnectsOnlyAfterContinue() {
        let app = isolatedApp()
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
        rows.typeText("127.0.0.1:33445")
        let next = app.buttons["network-continue"]
        XCTAssertFalse(next.isEnabled)
        app.buttons["network-check"].tap()
        let status = app.staticTexts["network-check-status"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "compatible node answered"),
            object: status
        ),
        ], timeout: 15), .completed)
        XCTAssertTrue(next.isEnabled)
        next.tap()
        XCTAssertTrue(app.buttons["profile-save"].waitForExistence(timeout: 10))
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), peerID)
        XCTAssertTrue(app.staticTexts["app-ready"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["network-edit"].exists)
        XCTAssertFalse(app.buttons["network-save"].exists)
        app.terminate()
    }

    func testNetworkFailureRowsKeepTheirOrder() {
        let app = isolatedApp()
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
        rows.typeText("not-an-address\n127.0.0.1:33446\n127.0.0.1:33446\n127.0.0.1:33445/bad")
        app.buttons["network-check"].tap()
        let expected = ["Invalid address", "Unreachable", "Duplicate address", "Invalid peer ID"]
        for (index, word) in expected.enumerated() {
            let result = app.descendants(matching: .any)["network-row-\(index)"]
            XCTAssertTrue(result.waitForExistence(timeout: 15))
            let displayed = [result.label, result.value as? String ?? ""].joined(separator: " ")
            XCTAssertTrue(displayed.contains(word), displayed)
        }
        XCTAssertFalse(app.buttons["network-continue"].isEnabled)
        app.terminate()
    }

    func testWrongNetworkDisablesContinue() {
        let app = isolatedApp()
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
        rows.typeText("127.0.0.1:33445")
        let name = app.textFields["network-name"]
        name.tap()
        name.typeText("city")
        app.buttons["network-check"].tap()
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(result.waitForExistence(timeout: 15))
        let displayed = [result.label, result.value as? String ?? ""].joined(separator: " ")
        XCTAssertTrue(displayed.contains("Wrong network"), displayed)
        XCTAssertFalse(app.buttons["network-continue"].isEnabled)
        app.terminate()
    }

    func testCancelRetryAndRestartKeepDraft() {
        let app = isolatedApp()
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
        rows.typeText("127.0.0.1:33447")
        app.buttons["network-check"].tap()
        let cancel = app.buttons["network-cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        cancel.tap()
        let status = app.staticTexts["network-check-status"]
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value CONTAINS %@", "Check cancelled"), object: status
        ),
        ], timeout: 15), .completed)
        let retry = app.buttons["network-check"]
        XCTAssertEqual(retry.label, "Retry connection")
        retry.tap()
        let result = app.descendants(matching: .any)["network-row-0"]
        XCTAssertTrue(result.waitForExistence(timeout: 15))
        let displayed = [result.label, result.value as? String ?? ""].joined(separator: " ")
        XCTAssertTrue(displayed.contains("Unreachable"), displayed)
        XCTAssertFalse(app.buttons["network-continue"].isEnabled)

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

    /// Put `-Key value` defaults before the slot root: the argument domain reads the slot root as a
    /// key and takes the next argument as its value, and AppKit opens a stray value as a document
    /// instead of showing the window.
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

    private func shownPeerId(_ app: XCUIApplication) -> String {
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10))
        return peerId.value as? String ?? ""
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
        let app = isolatedApp()
        app.launchArguments.insert(contentsOf: ["-NSDoubleLocalizedStrings", "YES"], at: 0)
        app.launch()
        createIdentity(app)
        let save = app.buttons["network-save"]
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        let text = save.value as? String ?? save.label
        XCTAssertTrue(text.hasPrefix("Save draft") && text.count >= 2 * "Save draft".count, text)
        XCTAssertTrue(save.isHittable)
        app.terminate()
    }

    func testRussianLanguage() {
        let app = isolatedApp()
        app.launchArguments.insert(contentsOf: ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"], at: 0)
        app.launch()
        createIdentity(app)
        let save = app.buttons["network-save"]
        XCTAssertTrue(save.waitForExistence(timeout: 10))
        XCTAssertEqual(save.value as? String ?? save.label,
                       "\u{0421}\u{043e}\u{0445}\u{0440}\u{0430}\u{043d}\u{0438}\u{0442}\u{044c} "
                       + "\u{0447}\u{0435}\u{0440}\u{043d}\u{043e}\u{0432}\u{0438}\u{043a}")
        app.terminate()
    }

    func testHiddenReadyDoesNotSatisfyLaunchContract() {
        let app = isolatedApp()
        app.launchArguments += ["--break-ui"]
        app.launch()
        createIdentity(app)
        XCTAssertTrue(app.staticTexts["DYApp"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
        app.terminate()
    }
}
