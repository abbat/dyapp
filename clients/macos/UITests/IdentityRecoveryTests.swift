import XCTest

/// Identity states seeded into the isolated root. Create must not appear in any of them: it would
/// replace an identity the person still expects to get back.
@MainActor
final class IdentityRecoveryTests: XCTestCase {
    func testLegacyRootIdentityKeepsItsPeerId() { assertStablePeerId("legacy") }

    func testMalformedPointerOpensTheSoleSlot() { assertStablePeerId("malformed-pointer") }

    func testMissingSlotKeyShowsRecovery() { assertRecovery("missing-slot-key") }

    func testMissingLegacyKeyShowsRecovery() { assertRecovery("missing-legacy-key") }

    func testPendingSlotShowsRecovery() { assertRecovery("pending-slot") }

    private func fixtureApp(_ fixture: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-identity-fixture=\(fixture)"]
        return app
    }

    private func shownPeerId(_ app: XCUIApplication) -> String {
        let peerId = app.staticTexts["peer-id"]
        XCTAssertTrue(peerId.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.buttons["identity-create"].exists)
        return peerId.value as? String ?? ""
    }

    private func assertStablePeerId(_ fixture: String) {
        let app = fixtureApp(fixture)
        app.launch()
        let first = shownPeerId(app)
        XCTAssertNotNil(first.range(of: "^[0-9a-f]{64}$", options: .regularExpression), first)
        app.terminate()
        app.launch()
        XCTAssertEqual(shownPeerId(app), first)
        app.terminate()
    }

    private func assertRecovery(_ fixture: String) {
        let app = fixtureApp(fixture)
        app.launch()
        let recovery = app.staticTexts["identity-recovery"]
        XCTAssertTrue(recovery.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.buttons["identity-create"].exists)
        app.buttons["identity-retry"].click()
        XCTAssertTrue(recovery.waitForExistence(timeout: 10))
        app.terminate()
        app.launch()
        XCTAssertTrue(recovery.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertFalse(app.buttons["identity-create"].exists)
        app.terminate()
    }
}
