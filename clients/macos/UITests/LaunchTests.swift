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
        XCTAssertNotNil(
            (peerId.value as? String ?? "").range(
                of: "^[0-9a-f]{64}$", options: .regularExpression), "Unexpected peer ID")
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
        let network = app.buttons["settings-network"]
        XCTAssertTrue(network.exists)
        // During setup the sidebar is the network form, so the link only closes the settings.
        network.click()
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "exists == false"), object: selected
                    )
                ], timeout: 10), .completed, app.debugDescription)
        XCTAssertTrue(app.buttons["network-save"].exists)
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
        createAnotherIdentity(app)
        let peer = app.staticTexts["peer-id"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value != %@", first), object: peer
                    )
                ], timeout: 10), .completed)
        let second = shownPeerId(app)
        app.buttons["settings-open"].tap()
        let remove = app.buttons["settings-remove"]
        reveal(remove, in: app)
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        remove.tap()
        let alert = confirmation(app, title: "Remove identity?")
        XCTAssertTrue(
            alert.staticTexts.matching(
                NSPredicate(
                    format: "label CONTAINS %@ OR value CONTAINS %@", second, second
                )
            ).firstMatch.exists)
        alert.buttons["Keep identity"].tap()
        XCTAssertTrue(remove.exists)
        remove.tap()
        alert.buttons["Remove permanently"].tap()
        XCTAssertTrue(app.staticTexts["identity-picker"].waitForExistence(timeout: 10))
        let select = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@", "identity-picker-select-"
            )
        ).firstMatch
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
        russian.launchArguments.insert(
            contentsOf: ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"], at: 0)
        russian.launch()
        createIdentity(russian)
        russian.buttons["settings-open"].tap()
        let remove = russian.buttons["settings-remove"]
        reveal(remove, in: russian)
        XCTAssertTrue(remove.waitForExistence(timeout: 10))
        XCTAssertEqual(
            remove.label,
            "\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{44d}\u{442}\u{443} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}"
        )
        remove.tap()
        let alert = confirmation(
            russian,
            title:
                "\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}?"
        )
        XCTAssertTrue(
            alert.buttons[
                "\u{423}\u{434}\u{430}\u{43b}\u{438}\u{442}\u{44c} \u{43d}\u{430}\u{432}\u{441}\u{435}\u{433}\u{434}\u{430}"
            ].exists)
        alert.buttons[
            "\u{41e}\u{441}\u{442}\u{430}\u{432}\u{438}\u{442}\u{44c} \u{438}\u{434}\u{435}\u{43d}\u{442}\u{438}\u{447}\u{43d}\u{43e}\u{441}\u{442}\u{44c}"
        ].tap()
        XCTAssertTrue(remove.exists)
        russian.terminate()

        let pseudo = isolatedApp()
        pseudo.launchArguments.insert(contentsOf: ["-NSDoubleLocalizedStrings", "YES"], at: 0)
        pseudo.launch()
        createIdentity(pseudo)
        pseudo.buttons["settings-open"].tap()
        let pseudoRemove = pseudo.buttons["settings-remove"]
        reveal(pseudoRemove, in: pseudo)
        XCTAssertTrue(pseudoRemove.waitForExistence(timeout: 10))
        let title = pseudoRemove.label
        XCTAssertTrue(title.hasPrefix("Remove this identity"))
        XCTAssertTrue(title.count >= 2 * "Remove this identity".count)
        XCTAssertTrue(pseudoRemove.isHittable)
        pseudoRemove.tap()
        let cancel = pseudo.sheets.buttons.matching(
            NSPredicate(
                format: "label BEGINSWITH %@", "Keep identity"
            )
        ).firstMatch
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
        XCTAssertTrue(
            selectedText.contains(
                "\u{41a}\u{43b}\u{44e}\u{447} "
                    + "\u{43d}\u{435}\u{434}\u{43e}\u{441}\u{442}\u{443}\u{43f}\u{435}\u{43d}"))
        // Rows are made only near the window, so the absence means something only down there.
        reveal(app.buttons["settings-create"], in: app)
        XCTAssertFalse(app.buttons["settings-remove"].exists)
        app.terminate()

        app.launch()
        let recovery = app.staticTexts["identity-recovery"]
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        let recoveryText = recovery.value as? String ?? recovery.label
        XCTAssertTrue(
            recoveryText.hasPrefix(
                "\u{41d}\u{435} "
                    + "\u{443}\u{434}\u{430}\u{43b}\u{43e}\u{441}\u{44c} "
                    + "\u{43e}\u{442}\u{43a}\u{440}\u{44b}\u{442}\u{44c}"))
        XCTAssertFalse(app.buttons["identity-create"].exists)
        app.buttons["identity-retry"].tap()
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        app.terminate()
    }
}
