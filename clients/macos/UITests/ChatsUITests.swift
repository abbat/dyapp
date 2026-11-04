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
    func testPeerRequestRepliesEditsClearsAndBlocks() {
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
        editAndDeleteTheReply(app, input: input)
        nameThePeer(app)
        muteClearAndBlock(app)
        toggleReceiptsAndUnblock(app)
    }

    /// The DEBUG peer likes this identity; passing is undone, and liking back opens a chat from the match.
    func testLikeBackFromLikedYouMatchesAndMessageOpensTheChat() {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer-likes"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        let open = app.buttons["chats-open"]
        XCTAssertTrue(open.waitForExistence(timeout: 30), app.debugDescription)
        open.click()

        let liked = app.buttons["chats-liked"]
        XCTAssertTrue(liked.waitForExistence(timeout: 10), app.debugDescription)
        liked.click()
        let row = app.buttons["liked-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 150), app.debugDescription)
        row.click()

        let pass = app.buttons["view-pass"]
        XCTAssertTrue(pass.waitForExistence(timeout: 30), app.debugDescription)
        reveal(pass, in: app)
        pass.click()
        let unpass = app.buttons["view-unpass"]
        XCTAssertTrue(unpass.waitForExistence(timeout: 30), app.debugDescription)
        unpass.click()

        let like = app.buttons["view-like"]
        XCTAssertTrue(like.waitForExistence(timeout: 30), app.debugDescription)
        reveal(like, in: app, deltaY: 300)
        like.click()
        XCTAssertTrue(app.descendants(matching: .any)["view-match"].waitForExistence(timeout: 60), app.debugDescription)
        app.buttons["view-message-match"].click()
        XCTAssertTrue(app.textFields["chat-input"].waitForExistence(timeout: 30), app.debugDescription)
    }

    private func editAndDeleteTheReply(_ app: XCUIApplication, input: XCUIElement) {
        menuItem("msg-edit", title: "Edit", in: app).click()
        XCTAssertTrue(app.staticTexts["chat-editing"].waitForExistence(timeout: 10), app.debugDescription)
        input.click()
        input.typeText(" again")
        app.buttons["chat-send"].click()
        // The field holds the same words until the edit lands, so its end comes first.
        waitGone(app.staticTexts["chat-editing"], in: app)
        let edited = shownText("Hello back again", in: app)
        XCTAssertTrue(edited.waitForExistence(timeout: 30), app.debugDescription)

        menuItem("msg-delete", title: "Delete", in: app).click()
        let confirm = app.sheets.buttons["Delete"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 10), app.debugDescription)
        confirm.click()
        waitGone(edited, in: app)
    }

    /// A local name shows in the chat's head and on its row.
    private func nameThePeer(_ app: XCUIApplication) {
        menuItem("chat-rename", title: "Name this contact", opener: "chat-more", in: app).click()
        let name = app.textFields["chat-rename-input"]
        XCTAssertTrue(name.waitForExistence(timeout: 10), app.debugDescription)
        name.click()
        name.typeText("Riley")
        app.buttons["chat-rename-save"].click()
        waitGone(name, in: app)
        XCTAssertTrue(shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)

        app.buttons["chat-back"].click()
        let row = app.buttons["chat-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)
        row.click()
        // The menu's items wait for the history.
        XCTAssertTrue(shownText("Hello from the peer", in: app).waitForExistence(timeout: 30), app.debugDescription)
    }

    private func muteClearAndBlock(_ app: XCUIApplication) {
        menuItem("chat-mute", title: "Mute notifications", opener: "chat-more", in: app).click()
        // The label follows the stored state.
        menuItem("chat-mute", title: "Unmute notifications", opener: "chat-more", in: app).click()

        menuItem("chat-clear", title: "Clear chat", opener: "chat-more", in: app).click()
        let clear = app.sheets.buttons["Clear chat"]
        XCTAssertTrue(clear.waitForExistence(timeout: 10), app.debugDescription)
        clear.click()
        waitGone(shownText("Hello from the peer", in: app), in: app)

        menuItem("chat-block", title: "Block", opener: "chat-more", in: app).click()
        let block = app.sheets.buttons["Block"]
        XCTAssertTrue(block.waitForExistence(timeout: 10), app.debugDescription)
        block.click()
        XCTAssertTrue(app.staticTexts["chats-blocked-note"].waitForExistence(timeout: 60), app.debugDescription)
    }

    /// Settings holds the receipts switch and the list the block above just filled.
    private func toggleReceiptsAndUnblock(_ app: XCUIApplication) {
        app.buttons["settings-open"].click()

        let receipts = app.checkBoxes["send-read-receipts"]
        XCTAssertTrue(receipts.waitForExistence(timeout: 30), app.debugDescription)
        reveal(receipts, in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: receipts
        ),
        ], timeout: 30), .completed, app.debugDescription)
        let wanted = receipts.value as? Int == 1 ? 0 : 1
        receipts.click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %d", wanted), object: receipts
        ),
        ], timeout: 30), .completed, app.debugDescription)

        let unblock = app.buttons["unblock-1"]
        reveal(unblock, in: app)
        XCTAssertTrue(unblock.waitForExistence(timeout: 30), app.debugDescription)
        reveal(unblock, in: app)
        unblock.click()
        XCTAssertTrue(app.staticTexts["blocked-none"].waitForExistence(timeout: 30), app.debugDescription)
    }

    private func waitGone(_ element: XCUIElement, in app: XCUIApplication) {
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [gone], timeout: 60), .completed, app.debugDescription)
    }

    /// Opens a menu. The app menu has a Delete of its own, so only the opened menu is searched.
    private func menuItem(
        _ id: String, title: String, opener: String = "msg-menu", in app: XCUIApplication
    ) -> XCUIElement {
        let menu = app.descendants(matching: .any)[opener].firstMatch
        // The chat's menu is off while its last action runs.
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"), object: menu
        ),
        ], timeout: 30), .completed, app.debugDescription)
        menu.click()
        let item = menu.descendants(matching: .menuItem).matching(
            NSPredicate(format: "identifier == %@ OR title == %@", id, title)
        ).firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10), app.debugDescription)
        return item
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
