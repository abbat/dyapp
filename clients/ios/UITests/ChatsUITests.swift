import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class ChatsUITests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

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
        XCTAssertTrue(
            app.staticTexts["chats-none"].waitForExistence(timeout: 10), app.debugDescription)
        requests.tap()
        XCTAssertTrue(
            app.staticTexts["chats-no-requests"].waitForExistence(timeout: 10), app.debugDescription
        )
        liked.tap()
        XCTAssertFalse(
            app.staticTexts["chats-none"].waitForExistence(timeout: 2),
            "a second tap closes the list")
    }

    /// The DEBUG peer writes first once this identity is listed; the reply reaches it, as its answer shows.
    func testPeerRequestRepliesEditsClearsAndBlocks() {
        let app = XCUIApplication()
        app.launchArguments = [
            "--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer",
        ]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()

        replyToTheRequest(app)
        XCTAssertTrue(
            shownText("Got your reply", in: app).waitForExistence(timeout: 150),
            app.debugDescription)
        menuItem("msg-forward", label: "Forward", opener: "msg-received-menu", in: app).tap()
        let empty = app.staticTexts["No other open chats are available."]
        XCTAssertTrue(empty.waitForExistence(timeout: 30))
        app.buttons["forward-cancel"].tap()
        waitGone(empty, in: app)
        editAndDeleteTheReply(app, input: app.textFields["chat-input"])
        nameThePeer(app)
        muteClearAndBlock(app)
        toggleReceiptsAndUnblock(app)
    }

    /// The DEBUG peer likes this identity; passing is undone, and liking back opens a chat from the match.
    func testLikeBackFromLikedYouMatchesAndMessageOpensTheChat() {
        let app = XCUIApplication()
        app.launchArguments = [
            "--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)",
            "--ui-test-chat-peer-likes", "--ui-test-metered-network",
        ]
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
        let drawn = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == 'Photo 1 of 1'"), object: photo)
        XCTAssertEqual(XCTWaiter.wait(for: [drawn], timeout: 60), .completed, app.debugDescription)
        let loadFull = reveal(app.buttons["view-photo-load"], in: app)
        loadFull.tap()
        waitGone(loadFull, in: app)

        assertPrivateFavoriteRoundTrip(app)
        assertPrivateNoteRoundTrip(app)
        reveal(app.buttons["view-pass"], in: app).tap()
        reveal(app.buttons["view-unpass"], in: app).tap()
        reveal(app.buttons["view-like"], in: app, above: true).tap()
        XCTAssertTrue(
            reveal(app.descendants(matching: .any)["view-match"], in: app, timeout: 60, above: true)
                .exists,
            app.debugDescription)
        reveal(app.buttons["view-message-match"], in: app).tap()
        XCTAssertTrue(
            app.textFields["chat-input"].waitForExistence(timeout: 30), app.debugDescription)
    }


    func testSharedProfileOpensFromItsCardAndSharesToAChat() {
        assertProfileShare(language: "en", locale: "en_US", sentLabel: "Profile sent.")
    }

    func testReceivedViewsInitiallyEmptyAndSendingDisabled() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        XCTAssertTrue(app.buttons["profile-preview"].waitForExistence(timeout: 30))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["settings-open"], in: app).tap()
        let preference = reveal(app.switches["send-profile-views"], in: app)
        guard ready(preference) else { return }
        XCTAssertEqual(preference.value as? String, "0")
        let history = reveal(app.buttons["settings-received-views"], in: app, fullyVisible: true)
        guard ready(history) else { return }
        history.tap()
        let empty = app.staticTexts["received-views-empty"]
        XCTAssertTrue(empty.waitForExistence(timeout: 30))
    }

    func testSharedProfileInRussian() {
        assertProfileShare(language: "ru", locale: "ru_RU", sentLabel: "Профиль отправлен.")
    }
}

extension ChatsUITests {
    /// From Chats: opens the peer's first message and answers it.
    func replyToTheRequest(_ app: XCUIApplication) {
        let requests = app.buttons["chats-requests"]
        XCTAssertTrue(requests.waitForExistence(timeout: 10), app.debugDescription)
        requests.tap()
        let request = app.buttons["request-1"]
        XCTAssertTrue(request.waitForExistence(timeout: 150), app.debugDescription)
        request.tap()
        XCTAssertTrue(
            app.staticTexts["chat-request-notice"].waitForExistence(timeout: 30),
            app.debugDescription)
        XCTAssertTrue(
            shownText("Hello from the peer", in: app).waitForExistence(timeout: 30),
            app.debugDescription)

        let input = app.textFields["chat-input"]
        input.tap()
        typeSlowly("Hello back", into: input)
        app.buttons["chat-send"].tap()
        XCTAssertTrue(
            shownText("Hello back", in: app).waitForExistence(timeout: 30), app.debugDescription)
    }

    private func editAndDeleteTheReply(_ app: XCUIApplication, input: XCUIElement) {
        menuItem("msg-edit", label: "Edit", in: app).tap()
        XCTAssertTrue(
            app.staticTexts["chat-editing"].waitForExistence(timeout: 10), app.debugDescription)
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
        XCTAssertTrue(
            shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)

        app.buttons["chat-back"].tap()
        let row = app.buttons["chat-1"]
        XCTAssertTrue(row.waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(
            shownText("Riley", in: app).waitForExistence(timeout: 30), app.debugDescription)
        row.tap()
        // The menu's items wait for the history.
        XCTAssertTrue(
            shownText("Hello from the peer", in: app).waitForExistence(timeout: 30),
            app.debugDescription)
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
        XCTAssertTrue(
            app.staticTexts["chats-blocked-note"].waitForExistence(timeout: 60),
            app.debugDescription)
    }

    /// Settings holds the receipts switch and the list the block above just filled.
    private func toggleReceiptsAndUnblock(_ app: XCUIApplication) {
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["settings-open"], in: app).tap()

        let receipts = app.switches["send-read-receipts"]
        reveal(receipts, in: app)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: receipts
                    )
                ], timeout: 30), .completed, app.debugDescription)
        let wanted = receipts.value as? String == "1" ? "0" : "1"
        flip(receipts, in: app, above: false)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "value == %@", wanted), object: receipts
                    )
                ], timeout: 30), .completed, app.debugDescription)

        reveal(app.buttons["unblock-1"], in: app).tap()
        XCTAssertTrue(
            app.staticTexts["blocked-none"].waitForExistence(timeout: 30), app.debugDescription)
    }

    private func waitGone(_ element: XCUIElement, in app: XCUIApplication) {
        let gone = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == false"), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [gone], timeout: 60), .completed, app.debugDescription)
    }

    private func menuItem(
        _ id: String, label: String, opener: String = "msg-menu", in app: XCUIApplication
    ) -> XCUIElement {
        let menu = app.buttons[opener].firstMatch
        // The chat's menu is off while its last action runs.
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "exists == true AND enabled == true"),
                        object: menu
                    )
                ], timeout: 30), .completed, app.debugDescription)
        menu.tap()
        let item = app.buttons.matching(
            NSPredicate(format: "identifier == %@ OR label == %@", id, label)
        ).firstMatch
        XCTAssertTrue(item.waitForExistence(timeout: 10), app.debugDescription)
        return item
    }

    /// A selectable text may carry its words in its value instead of its label.
    func shownText(_ text: String, in app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(
            NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text)
        ).firstMatch
    }

    func publishProfile(_ app: XCUIApplication) {
        if !app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10) {
            reveal(app.buttons["profile-edit"], in: app, timeout: 30).tap()
        }
        let month = app.textFields["profile-birth-month"]
        revealEditable(month, in: app)
        month.tap()
        month.typeText("5")
        reveal(app.buttons["profile-save"], in: app).tap()
        // The save took 15 s on a slow simulator before the switch turned on. Below the keyboard
        // the switch's row is not created, so a wait in place never sees it.
        let adult = reveal(app.switches["profile-confirm-adult"], in: app)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: adult
                    )
                ], timeout: 30), .completed, app.debugDescription)
        publish(app)
        let readback = app.descendants(matching: .any).matching(
            identifier: "profile-published-readback"
        ).firstMatch
        // The proof is lowered in UI tests but still has no time limit: a slow one would end in this wait.
        // The readback rows are created only once scrolled into view, so a wait in place can miss them.
        if !reveal(readback, in: app, timeout: 180).exists {
            // Below the fold: the nonce gives the proof's rate, the status row a publish still running.
            _ = reveal(
                app.descendants(matching: .any)["profile-proof-progress"], in: app, timeout: 5)
            XCTFail(app.debugDescription)
        }
    }

    func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
    }

    func connectLocalNode(_ app: XCUIApplication) {
        XCTAssertTrue(
            app.buttons["network-save"].waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"),
                        object: app.buttons["network-save"]
                    )
                ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        let status = reveal(app.staticTexts["network-check-status"], in: app)
        let answered =
            app.launchArguments.contains("(ru)")
            ? "Совместимый узел ответил." : "A compatible node answered."
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(
                            format: "label BEGINSWITH %@", answered),
                        object: status
                    )
                ], timeout: 30), .completed)
        let next = reveal(app.buttons["network-continue"], in: app)
        XCTAssertTrue(next.isEnabled)
        next.tap()
    }
}

extension ChatsUITests {
    private func assertPrivateFavoriteRoundTrip(_ app: XCUIApplication) {
        let peer = app.staticTexts["view-peer"].label
        favoriteButton(app, label: "Save privately").tap()
        _ = favoriteButton(app, label: "Remove from saved")
        XCTAssertTrue(reveal(app.buttons["view-like"], in: app).isEnabled)
        reveal(app.buttons["view-back"], in: app, above: true).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        openSavedCollection(app)
        let saved = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@", "saved-open-")
        ).firstMatch
        XCTAssertTrue(saved.waitForExistence(timeout: 30))
        saved.tap()
        XCTAssertTrue(app.staticTexts["view-peer"].waitForExistence(timeout: 30))
        XCTAssertEqual(app.staticTexts["view-peer"].label, peer)
        favoriteButton(app, label: "Remove from saved").tap()
        favoriteButton(app, label: "Save privately").tap()
        _ = favoriteButton(app, label: "Remove from saved")
        reveal(app.buttons["view-back"], in: app, above: true).tap()
        reveal(app.buttons["saved-profiles-toggle"], in: app, above: true).tap()
        let remove = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH %@", "saved-remove-")
        ).firstMatch
        XCTAssertTrue(remove.waitForExistence(timeout: 30))
        remove.tap()
        XCTAssertTrue(app.staticTexts["saved-profiles-empty"].waitForExistence(timeout: 30))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app).tap()
        reveal(app.buttons["liked-1"], in: app).tap()
        XCTAssertTrue(app.staticTexts["view-peer"].waitForExistence(timeout: 30))
        XCTAssertEqual(app.staticTexts["view-peer"].label, peer)
    }

    private func assertPrivateNoteRoundTrip(_ app: XCUIApplication) {
        let text = "Private Case\nLine two"
        let editor = app.textViews["view-note-text"]
        revealEditable(editor, in: app)
        guard ready(editor) else { return }
        editor.tap()
        typeSlowly(text, into: editor)
        awaitNoteValue(text, editor: editor)
        let save = reveal(app.buttons["view-note-save"], in: app)
        guard ready(save) else { return }
        save.tap()
        awaitNoteStatus("Private note saved.", in: app)
        reveal(app.buttons["view-back"], in: app, above: true).tap()
        reveal(app.buttons["liked-1"], in: app).tap()
        revealEditable(editor, in: app)
        guard ready(editor) else { return }
        awaitNoteValue(text, editor: editor)
        let clear = reveal(app.buttons["view-note-clear"], in: app)
        guard ready(clear) else { return }
        clear.tap()
        awaitNoteStatus("Private note cleared.", in: app)
        awaitNoteValue("", editor: editor)
    }

    private func awaitNoteValue(_ text: String, editor: XCUIElement) {
        let predicate =
            text.isEmpty
            ? NSPredicate(format: "enabled == true AND (value == '' OR value == nil)")
            : NSPredicate(format: "enabled == true AND value == %@", text)
        let completed = XCTNSPredicateExpectation(predicate: predicate, object: editor)
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 10), .completed)
    }

    private func awaitNoteStatus(_ label: String, in app: XCUIApplication) {
        let status = reveal(app.staticTexts["view-note-status"], in: app)
        let completed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@", label), object: status)
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 30), .completed)
    }

    private func favoriteButton(_ app: XCUIApplication, label: String) -> XCUIElement {
        let button = reveal(app.buttons["view-favorite-toggle"], in: app)
        let completed = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label == %@ AND enabled == true", label), object: button
        )
        XCTAssertEqual(XCTWaiter.wait(for: [completed], timeout: 30), .completed)
        return button
    }

    private func openSavedCollection(_ app: XCUIApplication) {
        let search = app.buttons["search-open"]
        XCTAssertTrue(search.waitForExistence(timeout: 10))
        search.tap()
        let toggle = reveal(app.buttons["saved-profiles-toggle"], in: app)
        XCTAssertTrue(toggle.waitForExistence(timeout: 10))
        toggle.tap()
    }
}
