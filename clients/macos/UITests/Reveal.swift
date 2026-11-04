import XCTest

/// Form rows outside the window are created only once scrolled into view; a positive
/// `deltaY` goes back up. A row that exists shows the way itself, so a long scroll past it
/// turns back instead of running on to the end of the form.
@MainActor
func reveal(_ element: XCUIElement, in app: XCUIApplication, deltaY: CGFloat = -300) {
    let form = app.scrollViews.firstMatch
    for _ in 0..<24 where !(element.exists && element.isHittable) {
        var step = deltaY
        if element.exists, element.frame.midY < form.frame.minY { step = 100 }
        if element.exists, element.frame.midY > form.frame.maxY { step = -100 }
        form.scroll(byDeltaX: 0, deltaY: step)
    }
}

/// Give both fresh confirmations, then start the proof and publish.
@MainActor
func publish(_ app: XCUIApplication) {
    for id in ["profile-confirm-adult", "profile-confirm-public"] {
        let toggle = app.checkBoxes[id]
        reveal(toggle, in: app)
        // A click on a window that lost key status only activates it and the toggle stays off.
        print("publish: \(id) app state \(app.state.rawValue) before activate (4 is foreground)")
        app.activate()
        toggle.click()
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == 1"), object: toggle
        ),
        ], timeout: 10), .completed)
    }
    let button = app.buttons["profile-prepare-publish"]
    reveal(button, in: app)
    XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
        predicate: NSPredicate(format: "enabled == true"), object: button
    ),
    ], timeout: 10), .completed)
    // The eligibility text above it changes length as the button enables and moves it.
    reveal(button, in: app)
    button.click()
}
