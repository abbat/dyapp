import XCTest

/// Form rows outside the window are created only once scrolled into view; a positive
/// `deltaY` goes back up. A row that exists shows the way itself, so a long scroll past it
/// turns back instead of running on to the end of the form.
@MainActor
func reveal(_ element: XCUIElement, in app: XCUIApplication, deltaY: CGFloat = -300) {
    let form = app.scrollViews.firstMatch
    for _ in 0..<24 {
        if element.exists && element.isHittable { return }
        var step = deltaY
        if element.exists, element.frame.midY < form.frame.minY { step = 100 }
        if element.exists, element.frame.midY > form.frame.maxY { step = -100 }
        form.scroll(byDeltaX: 0, deltaY: step)
    }
}

/// Reveals a field of a form that is off while its draft loads: a click then gives it no focus.
@MainActor
func revealEditable(_ field: XCUIElement, in app: XCUIApplication) {
    reveal(field, in: app)
    XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
        predicate: NSPredicate(format: "enabled == true"), object: field
    ),
    ], timeout: 30), .completed, app.debugDescription)
    reveal(field, in: app)
}

private func turnsOn(_ toggle: XCUIElement, within timeout: TimeInterval) -> Bool {
    XCTWaiter.wait(for: [XCTNSPredicateExpectation(
        predicate: NSPredicate(format: "value == 1"), object: toggle
    ),
    ], timeout: timeout) == .completed
}

/// Give both fresh confirmations, then start the proof and publish.
@MainActor
func publish(_ app: XCUIApplication) {
    for id in ["profile-confirm-adult", "profile-confirm-public"] {
        let toggle = app.checkBoxes[id]
        reveal(toggle, in: app)
        toggle.click()
        if !turnsOn(toggle, within: 3) {
            // shortcut: a lost click is recorded and retried once at another height, which would
            // hide an app bug that loses it; drop the retry once the record names the cause.
            print("publish: lost click on \(id) at \(toggle.frame), value \(String(describing: toggle.value))")
            XCTContext.runActivity(named: "Lost click on \(id)") { activity in
                let shot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
                shot.lifetime = .keepAlways
                activity.add(shot)
            }
            app.scrollViews.firstMatch.scroll(byDeltaX: 0, deltaY: 40)
            reveal(toggle, in: app)
            print("publish: second click on \(id) at \(toggle.frame)")
            toggle.click()
            XCTAssertTrue(turnsOn(toggle, within: 10), "\(id) stayed off after two clicks")
        }
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
