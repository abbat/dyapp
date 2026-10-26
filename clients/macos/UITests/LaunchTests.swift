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
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertEqual(ready.value as? String, "Ready") // macOS exposes Text in value; label is empty
        XCTAssertTrue(ready.isHittable)
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
        let link = app.buttons["network-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
    }

    /// Practice D1: Xcode's double-length pseudo-language; every string comes from the catalog.
    func testDoubleLengthPseudoLanguage() {
        let app = isolatedApp()
        app.launchArguments += ["-NSDoubleLocalizedStrings", "YES"]
        app.launch()
        createIdentity(app)
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        let text = ready.value as? String ?? ""
        XCTAssertTrue(text.hasPrefix("Ready") && text.count >= 2 * "Ready".count, text)
        XCTAssertTrue(ready.isHittable)
        app.terminate()
    }

    func testRussianLanguage() {
        let app = isolatedApp()
        app.launchArguments += ["-AppleLanguages", "(ru)", "-AppleLocale", "ru_RU"]
        app.launch()
        createIdentity(app)
        let ready = app.staticTexts["app-ready"]
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        XCTAssertEqual(ready.value as? String, "\u{0413}\u{043e}\u{0442}\u{043e}\u{0432}\u{043e}")
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
