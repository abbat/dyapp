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

    /// The DEBUG peer writes first once this identity is listed; the reply reaches it, as its answer shows.
    func testPeerRequestOpensAndTheReplyReachesThePeer() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        let open = app.buttons["chats-open"]
        XCTAssertTrue(open.waitForExistence(timeout: 30), app.debugDescription)
        open.click()

        let requests = app.buttons["chats-requests"]
        XCTAssertTrue(requests.waitForExistence(timeout: 10), app.debugDescription)
        requests.click()
        let request = app.buttons["request-1"]
        XCTAssertTrue(request.waitForExistence(timeout: 150), app.debugDescription)
        request.click()
        XCTAssertTrue(app.staticTexts["chat-request-notice"].waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(shownText("Hello from the peer", in: app).waitForExistence(timeout: 30), app.debugDescription)

        let input = app.textFields["chat-input"]
        input.click()
        input.typeText("Hello back")
        app.buttons["chat-send"].click()
        XCTAssertTrue(shownText("Hello back", in: app).waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(shownText("Got your reply", in: app).waitForExistence(timeout: 150), app.debugDescription)
    }

    /// A selectable text may carry its words in its value instead of its label.
    private func shownText(_ text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text)
        ).firstMatch
    }

    private func publishProfile(_ app: XCUIApplication) {
        if !app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10) {
            let link = app.buttons["profile-edit"]
            XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
            link.click()
        }
        let month = app.textFields["profile-birth-month"]
        reveal(month, in: app)
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
        let readback = app.descendants(matching: .any).matching(identifier: "profile-published-readback").firstMatch
        XCTAssertTrue(readback.waitForExistence(timeout: 60), app.debugDescription)
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
