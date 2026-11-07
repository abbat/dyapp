import XCTest

extension LaunchTests {
    func testSettingsCreatesAndSwitchesWithoutReplacingKeys() {
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
        XCTAssertNotEqual(second, first)
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
        app.buttons["settings-open"].tap()
        let select = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@", "settings-select-"
            )
        ).firstMatch
        XCTAssertTrue(select.waitForExistence(timeout: 10))
        select.tap()
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value == %@", first), object: peer
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
                ], timeout: 10), .completed)
        rows.tap()
        rows.typeText("one.example:33445\ntwo.example:7070")
        name.tap()
        name.typeText("friends")
        save.tap()
        let status = app.staticTexts["network-draft-status"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value CONTAINS %@", "Draft saved"),
                        object: status
                    )
                ], timeout: 10), .completed)
        XCTAssertTrue((status.value as? String ?? "").contains("Draft saved"))

        app.terminate()
        app.launch()
        openNetwork(app)
        XCTAssertFalse(app.staticTexts["app-ready"].exists)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"),
                        object: app.buttons["network-save"]
                    )
                ], timeout: 10), .completed)
        XCTAssertEqual(
            app.textViews["network-bootstrap"].value as? String,
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value CONTAINS %@", "could not be read"),
                        object: status
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
                ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        rows.typeText("127.0.0.1:33445")
        let next = app.buttons["network-continue"]
        XCTAssertFalse(next.isEnabled)
        app.buttons["network-check"].tap()
        let status = app.staticTexts["network-check-status"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(
                            format: "value CONTAINS %@", "compatible node answered"),
                        object: status
                    )
                ], timeout: 15), .completed)
        XCTAssertTrue(next.isEnabled)
        next.tap()
        // The form creates rows lazily: the save button is off screen, the first field is not.
        XCTAssertTrue(
            app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10))
        app.terminate()
        assertConnectedNetworkAfterRestart(app, peerID: peerID)
    }

    func assertConnectedNetworkAfterRestart(
        _ app: XCUIApplication, peerID: String
    ) {
        app.launch()
        XCTAssertEqual(shownPeerId(app), peerID)
        XCTAssertTrue(app.staticTexts["app-ready"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["network-edit"].exists)
        XCTAssertFalse(app.buttons["network-save"].exists)
        // The settings link selects the sidebar's network entry: a push in the detail column has no
        // way back on macOS 12. Another sidebar entry then replaces it.
        app.buttons["settings-open"].click()
        let network = app.buttons["settings-network"]
        XCTAssertTrue(network.waitForExistence(timeout: 10), app.debugDescription)
        network.click()
        XCTAssertTrue(
            app.buttons["network-save"].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.descendants(matching: .any)["settings-active"].exists)
        app.buttons["profile-edit"].click()
        XCTAssertTrue(
            app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["network-save"].exists)
        app.terminate()
    }

    func testNetworkFailureRowsKeepTheirOrder() {
        let app = isolatedApp()
        app.launch()
        createIdentity(app)
        openNetwork(app)
        let save = app.buttons["network-save"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: save
                    )
                ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        rows.typeText("127.0.0.1:33447")
        app.buttons["network-check"].tap()
        let cancel = app.buttons["network-cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 10))
        cancel.tap()
        let status = app.staticTexts["network-check-status"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value CONTAINS %@", "Check cancelled"),
                        object: status
                    )
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
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"),
                        object: app.buttons["network-save"]
                    )
                ], timeout: 10), .completed)
        XCTAssertEqual(app.textViews["network-bootstrap"].value as? String, "127.0.0.1:33447")
        app.terminate()
    }

    /// Put `-Key value` defaults before the slot root: the argument domain reads the slot root as a
    /// key and takes the next argument as its value, and AppKit opens a stray value as a document
    /// instead of showing the window.
    func isolatedApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)"]
        return app
    }

    func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
        // Settings appear only once the start screen has settled on its layout.
        XCTAssertTrue(
            app.buttons["settings-open"].waitForExistence(timeout: 10), app.debugDescription)
    }

    /// The button stays disabled until the identity list loads, and `tap()` does not wait for that.
    func createAnotherIdentity(_ app: XCUIApplication) {
        let create = app.buttons["settings-create"]
        reveal(create, in: app)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "isEnabled == true"), object: create
                    )
                ], timeout: 10), .completed, app.debugDescription)
        create.tap()
    }

    /// SwiftUI shows `.alert` on macOS as a window sheet whose title is a static text value.
    func confirmation(_ app: XCUIApplication, title: String) -> XCUIElement {
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(
            sheet.staticTexts.matching(
                NSPredicate(
                    format: "label == %@ OR value == %@", title, title
                )
            ).firstMatch.exists, sheet.debugDescription)
        return sheet
    }

    func shownPeerId(_ app: XCUIApplication) -> String {
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10))
        return peerId.value as? String ?? ""
    }

    func openNetwork(_ app: XCUIApplication) {
        if app.buttons["network-save"].waitForExistence(timeout: 10) { return }
        let link = app.buttons["network-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
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
