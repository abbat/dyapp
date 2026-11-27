import XCTest

final class RevealGeometryTests: XCTestCase {
    private let viewport = CGRect(x: 20, y: 100, width: 300, height: 500)
    private let labels = [
        CGRect(x: 40, y: 180, width: 120, height: 20),
        CGRect(x: 40, y: 480, width: 120, height: 20),
    ]

    @MainActor
    func testSettingsButtonProvidesANoneditableDragAnchor() {
        continueAfterFailure = false
        let app = unlockedApp()
        app.launch()
        defer { app.terminate() }
        createIdentity(app)
        app.buttons["settings-open"].tap()
        reveal(app.buttons["settings-network"], in: app)
        let visible = revealViewport(app)
        let anchors = revealAnchors(app, visible: visible)
        XCTAssertTrue(
            anchors.contains { $0.identity.hasPrefix("settings-network|") },
            "The current native button must be available to reveal virtual settings rows")
        reveal(app.buttons["settings-create"], in: app)
        XCTAssertTrue(app.buttons["settings-create"].isHittable)
    }

    func testUnknownRowsUseLiveLabelsAndViewportEdgesInEitherDirection() throws {
        let up = try XCTUnwrap(
            revealDrag(anchors: labels, visible: viewport, target: nil, above: false))
        XCTAssertEqual(up.start, CGPoint(x: 100, y: 490))
        XCTAssertEqual(up.end, CGPoint(x: 100, y: 100))
        let down = try XCTUnwrap(
            revealDrag(anchors: labels, visible: viewport, target: nil, above: true))
        XCTAssertEqual(down.start, CGPoint(x: 100, y: 190))
        XCTAssertEqual(down.end, CGPoint(x: 100, y: 600))
    }

    func testKnownRowsDetermineDirectionAndBoundTheActualTravel() throws {
        let target = CGRect(x: 40, y: 0, width: 100, height: 44)
        let drag = try XCTUnwrap(
            revealDrag(anchors: labels, visible: viewport, target: target, above: false))
        XCTAssertEqual(drag.start, CGPoint(x: 100, y: 190))
        XCTAssertEqual(drag.end, CGPoint(x: 100, y: 312))
        XCTAssertTrue(viewport.contains(drag.end))
    }

    func testAKeyboardViewportExcludesCoveredAnchorsAndEmptyGeometryCannotDrag() throws {
        let aboveKeyboard = CGRect(x: 20, y: 100, width: 300, height: 200)
        let drag = try XCTUnwrap(
            revealDrag(anchors: labels, visible: aboveKeyboard, target: nil, above: false))
        XCTAssertEqual(drag.start, CGPoint(x: 100, y: 190))
        XCTAssertEqual(drag.end.y, aboveKeyboard.minY)
        XCTAssertNil(revealDrag(anchors: [], visible: viewport, target: nil, above: false))
        XCTAssertNil(revealDrag(anchors: labels, visible: .zero, target: nil, above: false))
    }

    func testCoveredRowsMustMoveAndLargeRowsNeedTheirCentreVisible() {
        XCTAssertTrue(revealFits(labels[0], in: viewport))
        XCTAssertFalse(revealFits(CGRect(x: 40, y: 90, width: 120, height: 20), in: viewport))
        XCTAssertTrue(revealFits(CGRect(x: 40, y: -50, width: 120, height: 800), in: viewport))
        XCTAssertFalse(revealFits(CGRect(x: 40, y: 500, width: 120, height: 800), in: viewport))
    }

    func testPartialKeyboardRowsDoNotOscillateWithMeasuredNativeTravel() throws {
        // bca native trace: 190.6 requested/372 observed down, 183.6 requested/372 observed up.
        for scale in [0.8, 1.0, 2.0] {
            let visible = CGRect(x: 0, y: 97.6667 * scale, width: 393 * scale, height: 358 * scale)
            let anchor = CGRect(
                x: 40 * scale, y: 180 * scale, width: 120 * scale, height: 20 * scale)
            for (position, gain) in [(66.0, 372 / 190.6), (438.0, 372 / 183.6)] {
                let target = CGRect(
                    x: 20 * scale, y: position * scale, width: 353 * scale, height: 44 * scale)
                let drag = try XCTUnwrap(
                    revealDrag(anchors: [anchor], visible: visible, target: target, above: false))
                for response in [1.0, gain] {
                    let moved = target.offsetBy(dx: 0, dy: (drag.end.y - drag.start.y) * response)
                    XCTAssertTrue(
                        revealFits(moved, in: visible),
                        "The same row must fit after completed native travel")
                }
            }
        }
    }
}
