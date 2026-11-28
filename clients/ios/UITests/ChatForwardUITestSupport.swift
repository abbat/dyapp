import XCTest

extension ChatsUITests {
    func assertForwardedTextAndPhotoReachAnIndependentChat() {
        let app = XCUIApplication()
        app.launchArguments = [
            "--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-forward",
            "-AppleLanguages", "(en)", "-AppleLocale", "en_US",
        ]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()

        replyToTheRequest(app)
        XCTAssertTrue(shownText("Got your reply", in: app).waitForExistence(timeout: 150))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        openForwardDestination(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()

        openForwardChat("Photo", in: app)
        menuItem("msg-forward", label: "Forward", opener: "msg-received-menu", in: app).tap()
        selectForwardTarget(app)
        let photoMenu = app.buttons["msg-photo-menu"]
        guard ready(reveal(photoMenu, in: app, timeout: 150)) else { return }
        menuItem("msg-forward", label: "Forward", opener: "msg-photo-menu", in: app).tap()
        selectForwardTarget(app)

        app.navigationBars.buttons.element(boundBy: 0).tap()
        openForwardChat("Forwarded photo received", in: app)
        XCTAssertTrue(
            shownText("Forwarded \u{00B7} Hello from the peer", in: app).waitForExistence(timeout: 30))
        XCTAssertTrue(
            app.staticTexts["msg-photo-forwarded"].waitForExistence(timeout: 30))
    }

    private func selectForwardTarget(_ app: XCUIApplication) {
        let target = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "forward-target-")
        )
        .firstMatch
        guard ready(reveal(target, in: app, timeout: 30)) else { return }
        target.tap()
        waitGone(target, in: app)
        let cancel = app.buttons["forward-cancel"]
        if cancel.exists { cancel.tap() }
        waitGone(cancel, in: app)
    }

    private func openForwardDestination(_ app: XCUIApplication) {
        let request = app.buttons["request-1"]
        guard ready(reveal(request, in: app, timeout: 150)) else { return }
        request.tap()
        XCTAssertTrue(shownText("Forward destination", in: app).waitForExistence(timeout: 30))
        let input = app.textFields["chat-input"]
        input.tap()
        typeSlowly("Ready to receive a forward", into: input)
        app.buttons["chat-send"].tap()
        XCTAssertTrue(
            shownText("Ready to receive a forward", in: app).waitForExistence(timeout: 30))
    }

    private func openForwardChat(_ preview: String, in app: XCUIApplication) {
        let row = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH %@ AND label CONTAINS %@", "chat-", preview)
        ).firstMatch
        guard ready(reveal(row, in: app, timeout: 150)) else { return }
        row.tap()
    }
}
