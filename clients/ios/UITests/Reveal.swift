import XCTest

/// Form rows outside the screen are created only once scrolled into view, so drag the form up
/// (`above`: down, for a row above the screen) until `element` can be tapped or `timeout` passes,
/// with at least three drags.
@MainActor
@discardableResult
func reveal(_ element: XCUIElement, in app: XCUIApplication, timeout: TimeInterval = 30,
            above: Bool = false) -> XCUIElement {
    let deadline = Date().addingTimeInterval(timeout)
    // Drag in the upper half, where the keyboard of an edited field does not reach, and in the
    // right margin, so a text editor under the finger does not scroll its own text instead.
    let drag = { (from: CGFloat, to: CGFloat) in
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: from))
            .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: to)))
    }
    // One drag can take the whole timeout on a slow simulator, so a few always run.
    var drags = 0
    let search = {
        while !(element.exists && element.isHittable) && (Date() < deadline || drags < 3) {
            if above { drag(0.2, 0.35) } else { drag(0.35, 0.2) }
            drags += 1
        }
    }
    search()
    // A row under the navigation bar, or past the form's end above the App lock button and the
    // keyboard, still reports hittable, but a tap on its centre lands on what covers it. So move
    // it by its own frame. The form can jump back to its top on this drag and lose the row, so
    // search again; a later pass moves the row found after such a jump.
    for _ in 0..<3 where element.exists {
        let area = visibleArea(of: app)
        let centre = element.frame.midY
        guard centre < area.lowerBound + halfRow || centre > area.upperBound - halfRow else { break }
        print("reveal: \(element.identifier) at \(element.frame), visible y \(area), dragging")
        let span = area.upperBound - area.lowerBound - 2 * halfRow
        let distance = min(max(centre - (area.lowerBound + area.upperBound) / 2, -span), span)
        let start = distance > 0 ? area.upperBound - halfRow : area.lowerBound + halfRow
        let edge = app.coordinate(withNormalizedOffset: CGVector(dx: 0.97, dy: 0))
        edge.withOffset(CGVector(dx: 0, dy: start))
            .press(forDuration: 0.05, thenDragTo: edge.withOffset(CGVector(dx: 0, dy: start - distance)))
        search()
    }
    return element
}

/// A tap on a row's centre this close to the visible area's edge can land on what covers the row.
private let halfRow: CGFloat = 22

/// Where a form row can take a tap: the form runs under the navigation bar and ends above the App
/// lock button, and the keyboard can cover it too. Read it anew each time: the keyboard moves it.
@MainActor
private func visibleArea(of app: XCUIApplication) -> ClosedRange<CGFloat> {
    let bar = app.navigationBars.firstMatch
    let keyboard = app.keyboards.firstMatch
    // Older iOS builds a form from a table.
    let form = app.collectionViews.firstMatch.exists ? app.collectionViews.firstMatch : app.tables.firstMatch
    let top = bar.exists ? bar.frame.maxY : app.frame.minY
    let bottom = min(form.exists ? form.frame.maxY : app.frame.maxY,
                     keyboard.exists ? keyboard.frame.minY : app.frame.maxY)
    // A small area, as under a sheet: fall back to the whole screen rather than a broken range.
    return top < bottom ? top...bottom : app.frame.minY...app.frame.maxY
}

/// Reveals a field of a form that is off while its draft loads: a tap then gives it no focus.
@MainActor
func revealEditable(_ field: XCUIElement, in app: XCUIApplication) {
    reveal(field, in: app)
    XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
        predicate: NSPredicate(format: "enabled == true"), object: field
    ),
    ], timeout: 30), .completed, app.debugDescription)
    reveal(field, in: app)
}

/// Reveals a form switch and taps it: its row's centre is the label; newer iOS nests the tappable
/// switch inside the row, older iOS has none and the switch sits at the trailing edge.
@MainActor
func flip(_ toggle: XCUIElement, in app: XCUIApplication, above: Bool = false) {
    reveal(toggle, in: app, above: above)
    let knob = toggle.switches.firstMatch
    if knob.exists {
        knob.tap()
    } else {
        toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
    }
}

/// Give both fresh confirmations, then start the proof and publish.
@MainActor
func publish(_ app: XCUIApplication) {
    for id in ["profile-confirm-adult", "profile-confirm-public"] {
        let toggle = app.switches[id]
        reveal(toggle, in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: toggle
        ),
        ], timeout: 10), .completed)
        // The row's centre is its label; newer iOS nests the tappable switch inside the row,
        // older iOS has none and the switch sits at the trailing edge.
        let knob = toggle.switches.firstMatch
        if knob.exists {
            knob.tap()
        } else {
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        }
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '1'"), object: toggle
        ),
        ], timeout: 10), .completed)
    }
    let button = app.buttons["profile-prepare-publish"]
    reveal(button, in: app)
    XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
        predicate: NSPredicate(format: "enabled == true"), object: button
    ),
    ], timeout: 10), .completed)
    button.tap()
}

/// Fast typing into the SwiftUI text editor reorders characters on a slow simulator, so type
/// one character per call: each call first waits for the app to go idle.
@MainActor
func typeSlowly(_ text: String, into element: XCUIElement) {
    for character in text { element.typeText(String(character)) }
}
