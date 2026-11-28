import XCTest

extension ChatsUITests {
    func assertLinkDestinations(language: String, locale: String) {
        let app = XCUIApplication()
        app.launchArguments = [
            "--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-chat-peer",
            "-AppleLanguages", "(\(language))", "-AppleLocale", locale,
        ]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        publishProfile(app)
        app.navigationBars.buttons.element(boundBy: 0).tap()
        reveal(app.buttons["chats-open"], in: app, timeout: 30).tap()
        replyToTheRequest(app)
        XCTAssertTrue(shownText("Got your reply", in: app).waitForExistence(timeout: 150))

        let input = app.textFields["chat-input"]
        XCTAssertTrue(input.waitForExistence(timeout: 30))
        input.tap()
        input.typeText("[shown.example](https://xn--bcher-kva.example/)")
        app.buttons["chat-send"].tap()

        let hosts = app.staticTexts.matching(identifier: "chat-link-hosts")
            .matching(NSPredicate(format: "label == %@", "xn--bcher-kva.example (b\u{FC}cher.example)"))
            .firstMatch
        XCTAssertTrue(hosts.waitForExistence(timeout: 30))
    }
}
