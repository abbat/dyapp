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
        let app = isolatedApp()
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

        let more = app.checkBoxes["search-more"]
        reveal(more, in: app)
        more.click()
        let smoking = app.descendants(matching: .any)["search-field-smoking"]
        reveal(smoking, in: app)
        XCTAssertTrue(smoking.exists, app.debugDescription)

        let mutual = app.checkBoxes["search-mutual"]
        reveal(mutual, in: app, deltaY: 300)
        mutual.click()
        run(app)
        XCTAssertTrue(statusText(app).contains(mutualUnavailable), statusText(app))
        XCTAssertFalse(app.buttons["search-retry"].exists)
        app.terminate()
    }

    /// The launch argument puts one row ahead of the local node's answer, whose sample can be empty.
    func testFoundRowOpensItsProfileAndBackKeepsTheSample() {
        let app = isolatedApp()
        app.launchArguments.append("--ui-test-found-profile")
        app.launch()
        createIdentity(app)
        connectLocalNode(app)
        openSearch(app)
        run(app)
        let sample = statusText(app)

        let row = app.buttons["result-1"]
        reveal(row, in: app)
        row.click()
        let back = app.buttons["view-back"]
        XCTAssertTrue(back.waitForExistence(timeout: 10), app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["view-peer"].exists, app.debugDescription)
        XCTAssertTrue(app.descendants(matching: .any)["preview-name"].exists, app.debugDescription)
        XCTAssertFalse(app.buttons["run-search"].exists)

        back.click()
        reveal(row, in: app)
        XCTAssertTrue(row.exists, app.debugDescription)
        XCTAssertEqual(statusText(app), sample)
        app.terminate()
    }

    private func openSearch(_ app: XCUIApplication) {
        let link = app.buttons["search-open"]
        XCTAssertTrue(link.waitForExistence(timeout: 10), app.debugDescription)
        link.click()
        XCTAssertTrue(app.buttons["run-search"].waitForExistence(timeout: 10), app.debugDescription)
    }

    /// Click Search and wait for its finished status; a failed check is reported by the caller.
    private func run(_ app: XCUIApplication) {
        let button = app.buttons["run-search"]
        reveal(button, in: app)
        button.click()
        let status = app.staticTexts["search-status"]
        reveal(status, in: app)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(
                            format: "NOT (value CONTAINS %@) AND NOT (value CONTAINS %@)", idle,
                            loading),
                        object: status
                    )
                ], timeout: 60), .completed, app.debugDescription)
    }

    private func statusText(_ app: XCUIApplication) -> String {
        let status = app.staticTexts["search-status"]
        return status.value as? String ?? status.label
    }

    /// Replaces a filter's text; filters sit above the search button, so scroll up to reach them.
    private func type(_ text: String, into id: String, in app: XCUIApplication) {
        let field = app.textFields[id]
        reveal(field, in: app, deltaY: 300)
        field.click()
        field.typeKey("a", modifierFlags: .command)
        field.typeKey(.delete, modifierFlags: [])
        if !text.isEmpty { field.typeText(text) }
    }

    private func connectLocalNode(_ app: XCUIApplication) {
        openNetwork(app)
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"),
                        object: app.buttons["network-save"]
                    )
                ], timeout: 10), .completed)
        let rows = app.textViews["network-bootstrap"]
        rows.click()
        rows.typeText("127.0.0.1:33445")
        app.buttons["network-check"].click()
        let next = app.buttons["network-continue"]
        XCTAssertEqual(
            XCTWaiter.wait(
                for: [
                    XCTNSPredicateExpectation(
                        predicate: NSPredicate(format: "enabled == true"), object: next
                    )
                ], timeout: 15), .completed)
        next.click()
    }

    private func isolatedApp() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["--ui-test-slot-root=\(UUID().uuidString)"]
        return app
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
