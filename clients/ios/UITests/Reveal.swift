import XCTest

/// Form rows below the screen are created only once scrolled into view, so drag the form up
/// until `element` can be tapped or `timeout` passes.
@MainActor
@discardableResult
func reveal(_ element: XCUIElement, in app: XCUIApplication, timeout: TimeInterval = 10) -> XCUIElement {
    let deadline = Date().addingTimeInterval(timeout)
    while !(element.exists && element.isHittable) && Date() < deadline {
        // Drag in the upper half: the keyboard covers the lower one while a field is edited.
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            .press(forDuration: 0.05, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)))
    }
    return element
}

/// Fast typing into the SwiftUI text editor reorders characters on a slow simulator, so type
/// one character per call: each call first waits for the app to go idle.
@MainActor
func typeSlowly(_ text: String, into element: XCUIElement) {
    for character in text { element.typeText(String(character)) }
}
