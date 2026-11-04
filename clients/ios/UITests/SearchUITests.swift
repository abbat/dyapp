import XCTest

// XCUIApplication is main-actor isolated.
@MainActor
final class SearchUITests: XCTestCase {
    private let idle = "Search for a sample"
    private let loading = "Asking search nodes"
    private let ageInvalid = "Enter an age range"
    private let heightInvalid = "Enter a height range"
    private let mutualUnavailable = "Mutual search needs"

    /// A sample from the local node offers its filters; bad ranges and a mutual search without a
    /// saved profile name what to fix and offer no retry, and a valid range still searches.
    func testSampleOffersFiltersThatRefuseBadRanges() {
        let app = XCUIApplication()
        // Every launch starts with the app lock skipped, so the start screen renders.
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openSearch(app)
        XCTAssertFalse(app.textFields["search-age-min"].exists, "filters wait for a first sample")

        run(app)
        XCTAssertFalse(statusText(app).contains("No search node"), statusText(app))

        type("30", into: "search-age-min", in: app)
        type("20", into: "search-age-max", in: app)
        run(app)
        XCTAssertTrue(statusText(app).contains(ageInvalid), statusText(app))
        XCTAssertFalse(app.buttons["search-retry"].exists)

        type("", into: "search-age-min", in: app)
        type("60", into: "search-age-max", in: app)
        type("190", into: "search-height-min", in: app)
        type("170", into: "search-height-max", in: app)
        run(app)
        XCTAssertTrue(statusText(app).contains(heightInvalid), statusText(app))

        type("", into: "search-height-min", in: app)
        type("200", into: "search-height-max", in: app)
        run(app)
        XCTAssertFalse(statusText(app).contains(ageInvalid), statusText(app))
        XCTAssertFalse(statusText(app).contains(heightInvalid), statusText(app))

        flip(app.switches["search-more"], in: app, above: false)
        let smoking = app.descendants(matching: .any)["search-field-smoking"]
        XCTAssertTrue(reveal(smoking, in: app).exists, app.debugDescription)

        flip(app.switches["search-mutual"], in: app, above: true)
        run(app)
        XCTAssertTrue(statusText(app).contains(mutualUnavailable), statusText(app))
        XCTAssertFalse(app.buttons["search-retry"].exists)
        app.terminate()
    }

    /// The launch argument puts one row ahead of the local node's answer, whose sample can be empty.
    func testFoundRowOpensItsProfileAndBackKeepsTheSample() {
        let app = XCUIApplication()
        app.launchArguments = ["--app-lock-off", "--ui-test-slot-root=\(UUID().uuidString)", "--ui-test-found-profile"]
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openSearch(app)
        run(app)
        let sample = statusText(app)

        reveal(app.buttons["result-1"], in: app).tap()
        let back = app.buttons["view-back"]
        XCTAssertTrue(back.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["view-peer"].exists, app.debugDescription)
        XCTAssertTrue(reveal(app.descendants(matching: .any)["preview-name"], in: app).exists, app.debugDescription)
        XCTAssertFalse(app.buttons["run-search"].exists)

        reveal(back, in: app, above: true).tap()
        XCTAssertTrue(reveal(app.buttons["result-1"], in: app).exists, app.debugDescription)
        XCTAssertEqual(statusText(app), sample)
        app.terminate()
    }

    /// Continue opens the profile form over the start screen, so step back to the search link.
    private func openSearch(_ app: XCUIApplication) {
        if app.descendants(matching: .any)["profile-field-gender"].waitForExistence(timeout: 10) {
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
        let link = app.buttons["search-open"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(reveal(app.buttons["run-search"], in: app).exists, app.debugDescription)
    }

    /// Tap Search and wait for its finished status; a failed check is reported by the caller.
    private func run(_ app: XCUIApplication) {
        reveal(app.buttons["run-search"], in: app).tap()
        let status = reveal(app.staticTexts["search-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "NOT (label CONTAINS %@) AND NOT (label CONTAINS %@)", idle, loading),
            object: status
        ),
        ], timeout: 60), .completed, app.debugDescription)
    }

    private func statusText(_ app: XCUIApplication) -> String {
        app.staticTexts["search-status"].label
    }

    /// Replaces a filter's text; filters sit above the search button, so drag down to reach them.
    private func type(_ text: String, into id: String, in app: XCUIApplication) {
        let field = reveal(app.textFields[id], in: app, above: true)
        field.tap()
        // An empty field reports its placeholder, and deleting past the start does nothing.
        let old = field.value as? String ?? ""
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: old.count) + text)
    }

    /// The row's centre is its label; newer iOS nests the tappable switch inside the row, older
    /// iOS has none and the switch sits at the trailing edge.
    private func flip(_ toggle: XCUIElement, in app: XCUIApplication, above: Bool) {
        reveal(toggle, in: app, above: above)
        let knob = toggle.switches.firstMatch
        if knob.exists {
            knob.tap()
        } else {
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        }
    }

    private func connectLocalNode(_ app: XCUIApplication) {
        openNetwork(app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == true"), object: app.buttons["network-save"]
        ),
        ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.tap()
        typeSlowly("127.0.0.1:33445", into: rows)
        reveal(app.buttons["network-check"], in: app).tap()
        // Reveal Continue only after the check answers: its rows move Continue while they appear.
        let status = reveal(app.staticTexts["network-check-status"], in: app)
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "label CONTAINS %@", "compatible node answered"),
            object: status
        ),
        ], timeout: 30), .completed)
        let next = reveal(app.buttons["network-continue"], in: app)
        XCTAssertTrue(next.isEnabled)
        next.tap()
    }

    private func createIdentity(_ app: XCUIApplication) {
        let create = app.buttons["identity-create"]
        XCTAssertTrue(create.waitForExistence(timeout: 10), app.debugDescription)
        if create.exists { create.tap() }
    }

    private func openNetwork(_ app: XCUIApplication) {
        if app.buttons["network-save"].waitForExistence(timeout: 10) { return }
        let link = app.buttons["network-edit"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.tap()
        XCTAssertTrue(app.buttons["network-save"].waitForExistence(timeout: 10))
    }
}
