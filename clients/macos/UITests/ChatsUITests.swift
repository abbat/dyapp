import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class ChatsUITests: XCTestCase {
    /// A fresh identity has empty lists, and the row counts come without any fetch.
    func testNewIdentityHasEmptyLists() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        let open = app.buttons["chats-open"]
        XCTAssertTrue(open.waitForExistence(timeout: 30), app.debugDescription)
        open.click()

        let empty = app.staticTexts["chats-empty"]
        XCTAssertTrue(empty.waitForExistence(timeout: 10), app.debugDescription)
        let liked = app.buttons["chats-liked"]
        let requests = app.buttons["chats-requests"]
        XCTAssertEqual(liked.value as? String, "0")
        XCTAssertEqual(requests.value as? String, "0")

        liked.click()
        XCTAssertTrue(app.staticTexts["chats-none"].waitForExistence(timeout: 10), app.debugDescription)
        requests.click()
        XCTAssertTrue(app.staticTexts["chats-no-requests"].waitForExistence(timeout: 10), app.debugDescription)
        liked.click()
        XCTAssertFalse(app.staticTexts["chats-none"].waitForExistence(timeout: 2), "a second click closes the list")
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
}
