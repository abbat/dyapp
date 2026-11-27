import XCTest

extension ChatsUITests {
    func assertProfileShare(language: String, locale: String, sentLabel: String) {
        let app = XCUIApplication()
        let configuration = [
            "-AppleLanguages", "(\(language))", "-AppleLocale", locale,
            "-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityM",
        ]
        app.launchArguments =
            [
                "--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)",
                "--ui-test-chat-peer-shares",
            ] + configuration
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
        let share = reveal(app.buttons["view-share"], in: app)
        XCTAssertEqual(share.value as? String, "accessibility")
        share.tap()
        let target = reveal(app.buttons["share-1"], in: app, fullyVisible: true)
        guard ready(target) else { return }
        attachShareState(
            "\(language)-eligible-target", app: app, element: target, configuration: configuration)
        target.tap()
        let sent = app.staticTexts.matching(identifier: "view-status").matching(
            NSPredicate(format: "label == %@", sentLabel)
        ).firstMatch
        reveal(sent, in: app, timeout: 60, above: true, fullyVisible: true)
        attachShareState("\(language)-sent", app: app, element: sent, configuration: configuration)
        reveal(app.buttons["view-back"], in: app, above: true).tap()
        XCTAssertTrue(cards.element(boundBy: 1).waitForExistence(timeout: 30), app.debugDescription)
        XCTAssertTrue(
            shownText("Shared profile received", in: app).waitForExistence(timeout: 150),
            "The distinct peer must receive the exact owner link once")
    }

    private func attachShareState(
        _ state: String, app: XCUIApplication, element: XCUIElement, configuration: [String]
    ) {
        XCTAssertTrue(element.exists && element.isHittable)
        let frame = element.frame
        let viewport = app.frame
        XCTAssertTrue(!frame.isEmpty && viewport.contains(frame))
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "ios-profile-share-\(state)"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        let requested = configuration.joined(separator: " ")
        let geometry = XCTAttachment(
            string: "element=\(frame) viewport=\(viewport) requested: \(requested)")
        geometry.name = "ios-profile-share-\(state)-geometry"
        geometry.lifetime = .keepAlways
        add(geometry)
    }
}
