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
    while !(element.exists && element.isHittable) && (Date() < deadline || drags < 3) {
        if above { drag(0.2, 0.35) } else { drag(0.35, 0.2) }
        drags += 1
    }
    // A row half under the bottom bar still reports hittable, but a tap on its centre misses.
    if element.exists && element.frame.maxY > app.frame.maxY * 0.8 { drag(0.35, 0.2) }
    return element
}

/// Fast typing into the SwiftUI text editor reorders characters on a slow simulator, so type
/// one character per call: each call first waits for the app to go idle.
@MainActor
func typeSlowly(_ text: String, into element: XCUIElement) {
    for character in text { element.typeText(String(character)) }
}
