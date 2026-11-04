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

    /// The DEBUG peer writes first once this identity is listed; the reply reaches it, as its answer shows.
    func testPeerRequestRepliesEditsClearsAndBlocks() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()

        replyToTheRequest(app)
        XCTAssertTrue(shownText("Got your reply", in: app).waitForExistence(timeout: 150), app.debugDescription)
        editAndDeleteTheReply(app, input: app.textFields["chat-input"])
        nameThePeer(app)
        muteClearAndBlock(app)
        toggleReceiptsAndUnblock(app)
    }

    /// The DEBUG peer likes this identity; passing is undone, and liking back opens a chat from the match.
    func testLikeBackFromLikedYouMatchesAndMessageOpensTheChat() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer-likes", "--ui-test-metered-network"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()

        let liked = app.buttons["chats-liked"]
        XCTAssertTrue(liked.waitForExistence(timeout: 10), app.debugDescription)
        liked.tap()
        let row = app.buttons["liked-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 150), app.debugDescription)
        row.tap()

        XCTAssertTrue(app.buttons["view-back"].waitForExistence(timeout: 30), app.debugDescription)
        // Only a drawn photo has this label; the full image then ends the offer to load it.
        let photo = reveal(app.descendants(matching: .any)["view-photo"], in: app)
        let drawn = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == 'Photo 1 of 1'"), object: photo)
        XCTAssertEqual(XCTWaiter.wait(for: [drawn], timeout: 60), .completed, app.debugDescription)
        let loadFull = reveal(app.buttons["view-photo-load"], in: app)
        loadFull.tap()
        waitGone(loadFull, in: app)

        reveal(app.buttons["view-pass"], in: app).tap()
        reveal(app.buttons["view-unpass"], in: app).tap()
        reveal(app.buttons["view-like"], in: app, above: true).tap()
        XCTAssertTrue(reveal(app.descendants(matching: .any)["view-match"], in: app, timeout: 60, above: true).exists,
                      app.debugDescription)
        reveal(app.buttons["view-message-match"], in: app).tap()
        XCTAssertTrue(app.textFields["chat-input"].waitForExistence(timeout: 30), app.debugDescription)
    }

    /// The DEBUG peer answers with its profile link; its card opens the page, whose Share sends the link back.
    func testSharedProfileOpensFromItsCardAndSharesToAChat() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer-shares"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()
        replyToTheRequest(app)

        let cards = app.buttons.matching(identifier: "msg-profile")
        XCTAssertTrue(cards.firstMatch.waitForExistence(timeout: 150), app.debugDescription)
        cards.firstMatch.tap()
        XCTAssertTrue(app.buttons["view-back"].waitForExistence(timeout: 30), app.debugDescription)
        reveal(app.buttons["view-share"], in: app).tap()
        reveal(app.buttons["share-1"], in: app).tap()
        XCTAssertTrue(reveal(shownText("Profile sent", in: app), in: app, timeout: 60, above: true).exists,
                      app.debugDescription)
        reveal(app.buttons["view-back"], in: app, above: true).tap()
        XCTAssertTrue(cards.element(boundBy: 1).waitForExistence(timeout: 30), app.debugDescription)
    }

    /// From Chats: opens the peer's first message and answers it.
    private func replyToTheRequest(_ app: XCUIApplication) {
        let requests = app.buttons["chats-requests"]
        XCTAssertTrue(requests.waitForExistence(timeout: 10), app.debugDescription)
        requests.tap()
        let request = app.buttons["request-1"]
        XCTAssertTrue(request.waitForExistence(timeout: 150), app.debugDescription)
        request.tap()
        XCTAssertTrue(app.staticTexts["chat-request-notice"].waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(shownText("Hello from the peer", in: app).waitForExistence(timeout: 30), app.debugDescription)

        let input = app.textFields["chat-input"]
        input.tap()
        typeSlowly("Hello back", into: input)
        app.buttons["chat-send"].tap()
        XCTAssertTrue(shownText("Hello back", in: app).waitForExistence(timeout: 30), app.debugDescription)
    }

    private func editAndDeleteTheReply(_ app: XCUIApplication, input: XCUIElement) {
        menuItem("msg-edit", label: "Edit", in: app).tap()
        XCTAssertTrue(app.staticTexts["chat-editing"].waitForExistence(timeout: 10), app.debugDescription)
        input.tap()
        typeSlowly(" again", into: input)
        app.buttons["chat-send"].tap()
        // The field holds the same words until the edit lands, so its end comes first.
        waitGone(app.staticTexts["chat-editing"], in: app)
        let edited = shownText("Hello back again", in: app)
        XCTAssertTrue(edited.waitForExistence(timeout: 30), app.debugDescription)

        menuItem("msg-delete", label: "Delete", in: app).tap()
        let confirm = app.buttons["Delete"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 10), app.debugDescription)
        confirm.tap()
        waitGone(edited, in: app)
    }

    /// A local name shows in the chat's head and on its row.
    private func nameThePeer(_ app: XCUIApplication) {
        menuItem("chat-rename", label: "Name this contact", opener: "chat-more", in: app).tap()
        let name = app.textFields["chat-rename-input"]
        XCTAssertTrue(name.waitForExistence(timeout: 10), app.debugDescription)
        name.tap()
        typeSlowly("Riley", into: name)
        app.buttons["chat-rename-save"].tap()
        waitGone(name, in: app)
        XCTAssertTrue(shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)

        app.buttons["chat-back"].tap()
        let row = app.buttons["chat-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)
        row.tap()
        // The menu's items wait for the history.
        XCTAssertTrue(shownText("Hello from the peer", in: app).waitForExistence(timeout: 30), app.debugDescription)
    }

    private func muteClearAndBlock(_ app: XCUIApplication) {
        menuItem("chat-safety", label: "Compare safety number", opener: "chat-more", in: app).tap()
        let number = app.staticTexts["safety-number"]
        XCTAssertTrue(number.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.images["safety-qr"].exists, app.debugDescription)
        app.buttons["safety-done"].tap()
        waitGone(number, in: app)
        menuItem("chat-mute", label: "Mute notifications", opener: "chat-more", in: app).tap()
        // The label follows the stored state.
        menuItem("chat-mute", label: "Unmute notifications", opener: "chat-more", in: app).tap()

        menuItem("chat-clear", label: "Clear chat", opener: "chat-more", in: app).tap()
        let clear = app.buttons["Clear chat"].firstMatch
        XCTAssertTrue(clear.waitForExistence(timeout: 10), app.debugDescription)
        clear.tap()
        waitGone(shownText("Hello from the peer", in: app), in: app)

        menuItem("chat-block", label: "Block", opener: "chat-more", in: app).tap()
        let block = app.buttons["Block"].firstMatch
        XCTAssertTrue(block.waitForExistence(timeout: 10), app.debugDescription)
        block.tap()
        XCTAssertTrue(app.staticTexts["chats-blocked-note"].waitForExistence(timeout: 60), app.debugDescription)
    }

    /// Settings holds the receipts switch and the list the block above just filled.
    private func toggleReceiptsAndUnblock(_ app: XCUIApplication) {
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["settings-open"], in: app).tap()

        let receipts = app.switches["send-read-receipts"]
        reveal(receipts, in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: receipts
        ),
        ], timeout: 30), .completed, app.debugDescription)
        let wanted = receipts.value as? String == "1" ? "0" : "1"
        flip(receipts, in: app, above: false)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == %@", wanted), object: receipts
        ),
        ], timeout: 30), .completed, app.debugDescription)

        reveal(app.buttons["unblock-1"], in: app).tap()
        XCTAssertTrue(app.staticTexts["blocked-none"].waitForExistence(timeout: 30), app.debugDescription)
    }

    private func waitGone(_ element: XCUIElement, in app: XCUIApplication) {
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [gone], timeout: 60), .completed, app.debugDescription)
    }

    private func menuItem(
        _ id: String, label: String, opener: String = "msg-menu", in app: XCUIApplication
    ) -> XCUIElement {
        let menu = app.buttons[opener].firstMatch
        // The chat's menu is off while its last action runs.
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true"), object: menu
        ),
        ], timeout: 30), .completed, app.debugDescription)
        menu.tap()
        let item = app.buttons.matching(NSPredicate(format: "identifier == %@ OR label == %@", id, label)).firstMatch
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
            reveal(app.buttons["profile-edit"], in: app, timeout: 30).tap()
        }
        let month = app.textFields["profile-birth-month"]
        reveal(month, in: app)
        month.tap()
        month.typeText("5")
        reveal(app.buttons["profile-save"], in: app).tap()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.switches["profile-confirm-adult"]
        ),
        ], timeout: 10), .completed)
        publish(app)
        let readback = app.descendants(matching: .any).matching(identifier: "profile-published-readback").firstMatch
        // The proof has no time limit and only these tests fail 60 s: a slow proof would end in this wait.
        if !readback.waitForExistence(timeout: 180) {
            // Below the fold: the nonce gives the proof's rate, the status row a publish still running.
            _ = reveal(app.descendants(matching: .any)["profile-proof-progress"], in: app, timeout: 5)
            XCTFail(app.debugDescription)
        }
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
