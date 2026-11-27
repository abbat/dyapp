import XCTest

/// Virtual rows are revealed with completed native gestures and fresh geometry per step.
@MainActor
@discardableResult
func reveal(
    _ element: XCUIElement, in app: XCUIApplication, timeout: TimeInterval = 30,
    above: Bool = false, fullyVisible: Bool = false
) -> XCUIElement {
    let row = element.firstMatch
    var deadline = ProcessInfo.processInfo.systemUptime + timeout
    var visited = Set<String>()
    while ProcessInfo.processInfo.systemUptime < deadline {
        let visible = revealViewport(app)
        let snapshot = row.exists ? (try? row.snapshot()) : nil
        let frame = snapshot?.frame
        let name = snapshot?.identifier ?? "unmaterialized target"
        // Hittability may describe only an exposed fragment. Editable controls must also
        // expose their actual tap area above the current keyboard before native entry.
        if row.exists && row.isHittable
            && (!fullyVisible || frame.map { revealFits($0, in: visible) } == true)
        {
            return element
        }
        if let frame, revealFits(frame, in: visible) {
            _ = ready(row, timeout: max(0, deadline - ProcessInfo.processInfo.systemUptime))
            return element
        }
        let anchors = revealAnchors(app, visible: visible)
        let state = "\(visible)|\(String(describing: frame))|\(anchors)"
        guard visited.insert(state).inserted,
            let drag = revealDrag(
                anchors: anchors.map(\.frame), visible: visible,
                target: frame.flatMap { $0.isEmpty ? nil : $0 },
                above: above)
        else {
            XCTFail(
                "Cannot reveal \(name): frame=\(String(describing: frame)), viewport=\(visible)"
            )
            return element
        }
        let origin = app.coordinate(withNormalizedOffset: .zero)
        let originFrame = app.frame
        func coordinate(_ point: CGPoint) -> XCUICoordinate {
            origin.withOffset(
                CGVector(dx: point.x - originFrame.minX, dy: point.y - originFrame.minY))
        }
        coordinate(drag.start).press(forDuration: 0, thenDragTo: coordinate(drag.end))
        let nextVisible = revealViewport(app)
        let nextFrame = row.exists ? (try? row.snapshot())?.frame : nil
        let next =
            "\(nextVisible)|\(String(describing: nextFrame))|\(revealAnchors(app, visible: nextVisible))"
        guard next != state else {
            XCTFail("Form made no scroll progress revealing \(name), viewport=\(visible)")
            return element
        }
        deadline = ProcessInfo.processInfo.systemUptime + timeout
    }
    XCTFail("Reveal deadline exceeded before the target became ready")
    return element
}

@MainActor
func ready(_ control: XCUIElement, timeout: TimeInterval = 10) -> Bool {
    let result = XCTWaiter.wait(
        for: [
            XCTNSPredicateExpectation(
                predicate: NSPredicate(
                    format: "exists == true AND enabled == true AND hittable == true"),
                object: control)
        ], timeout: timeout)
    XCTAssertEqual(result, .completed, "Control not ready: \(control.identifier)")
    return result == .completed
}

/// Draft loading, scrolling and focus readiness are independent completed transitions.
@MainActor
func revealEditable(_ field: XCUIElement, in app: XCUIApplication) {
    reveal(field, in: app, fullyVisible: true)
    guard ready(field, timeout: 30) else { return }
    reveal(field, in: app, fullyVisible: true)
}

/// Tap the actual native switch once; never retry a toggle that might hide a real state reset.
@MainActor
private func setSwitch(
    _ toggle: XCUIElement, value: String, in app: XCUIApplication, above: Bool = false
) -> Bool {
    reveal(toggle, in: app, above: above)
    guard ready(toggle) else { return false }
    if toggle.value as? String == value { return true }
    let nested = toggle.switches.firstMatch
    let control = nested.exists ? nested : toggle
    // A hittable row label does not establish that its native switch is exposed.
    reveal(control, in: app, above: above)
    guard ready(control) else { return false }
    control.tap()
    let result = XCTWaiter.wait(
        for: [
            XCTNSPredicateExpectation(
                predicate: NSPredicate(format: "value == %@", value), object: toggle)
        ], timeout: 10)
    XCTAssertEqual(
        result, .completed,
        "Switch \(toggle.identifier) did not reach \(value); row=\(String(describing: toggle.value)), control=\(String(describing: control.value))"
    )
    return result == .completed
}

@MainActor
func flip(_ toggle: XCUIElement, in app: XCUIApplication, above: Bool = false) {
    reveal(toggle, in: app, above: above)
    guard ready(toggle), let value = toggle.value as? String, ["0", "1"].contains(value) else {
        XCTFail("Switch has no ready binary state: \(toggle.identifier)")
        return
    }
    _ = setSwitch(toggle, value: value == "0" ? "1" : "0", in: app, above: above)
}

/// Confirm the required state, observe each completion, then start proof and publication.
@MainActor
func publish(_ app: XCUIApplication) {
    for id in ["profile-confirm-adult", "profile-confirm-public"] {
        guard setSwitch(app.switches[id], value: "1", in: app) else { return }
    }
    let button = app.buttons["profile-prepare-publish"]
    reveal(button, in: app)
    guard ready(button) else { return }
    button.tap()
}

/// Per-character native calls await app idleness; fast TextEditor typing can reorder input.
@MainActor
func typeSlowly(_ text: String, into element: XCUIElement) {
    for character in text { element.typeText(String(character)) }
}
