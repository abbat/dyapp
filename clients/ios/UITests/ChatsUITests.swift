import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class ChatsUITests: XCTestCase {
    /// A fresh identity has empty lists, and the row counts come without any fetch.
    func testNewIdentityHasEmptyLists() {
        let app = XCUIApplication()
        // Every launch starts with the app lock skipped, so the start screen renders.
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        // Setup goes on to the profile; its back button leads to the Ready screen.
        if app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10) {
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()

        let empty = app.staticTexts["chats-empty"]
        XCTAssertTrue(empty.waitForExistence(timeout: 10), app.debugDescription)
        let liked = app.buttons["chats-liked"]
        let requests = app.buttons["chats-requests"]
        XCTAssertEqual(liked.value as? String, "0")
        XCTAssertEqual(requests.value as? String, "0")

        liked.tap()
        XCTAssertTrue(app.staticTexts["chats-none"].waitForExistence(timeout: 10), app.debugDescription)
        requests.tap()
        XCTAssertTrue(app.staticTexts["chats-no-requests"].waitForExistence(timeout: 10), app.debugDescription)
        liked.tap()
        XCTAssertFalse(app.staticTexts["chats-none"].waitForExistence(timeout: 2), "a second tap closes the list")
    }

    private func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
    }

    private func connectLocalNode(_ app: XCUIApplication) {
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        // Reveal Continue only after the check answers: its rows move Continue while they appear.
        let status = reveal(app.staticTexts["network-check-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "compatible node answered"),
            object: status
        ),
        ], timeout: 30), .completed)
        let next = reveal(app.buttons["network-continue"], in: app)
        XCTAssertTrue(next.isEnabled)
        next.tap()
    }
}
