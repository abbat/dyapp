@testable import DYApp
import XCTest

@MainActor
final class SearchOwnershipTests: XCTestCase {
    func testSamePeerInDifferentSlotsRetiresItsOldRunAndFilters() throws {
        let session = SearchSession()
        let first = candidate()
        let second = candidate()
        session.open(first, peerId: "same peer")
        let oldOwner = session.owner
        let old = try XCTUnwrap(session.begin(owner: oldOwner))
        session.filters.country = "DE"
        session.open(second, peerId: "same peer")
        XCTAssertNotEqual(session.owner, oldOwner)
        XCTAssertNil(session.context(for: first, peerId: "same peer"))
        XCTAssertEqual(session.context(for: second, peerId: "same peer"), session.owner)
        XCTAssertTrue(session.filters.country.isEmpty)
        session.complete(outcome(offered: [46]), for: old)
        XCTAssertTrue(session.filters.offered.isEmpty)
        guard case .idle = session.state else {
            return XCTFail("An obsolete sample replaced the successor")
        }
        XCTAssertNil(session.begin(owner: oldOwner))
    }

    func testCancelledCompletionCannotReplaceAnIndependentlyAdmittedRetry() throws {
        let session = SearchSession()
        session.open(candidate(), peerId: "peer")
        let owner = session.owner
        let first = try XCTUnwrap(session.begin(owner: owner))
        XCTAssertNil(session.begin(owner: owner), "A pending request cannot be submitted twice")
        session.cancel(owner: owner)
        let retry = try XCTUnwrap(session.begin(owner: owner))
        XCTAssertNotEqual(first.id, retry.id)
        session.complete(outcome(offered: [46]), for: first)
        XCTAssertTrue(session.state.isLoading)
        XCTAssertTrue(session.filters.offered.isEmpty)
        session.complete(outcome(offered: [4]), for: retry)
        XCTAssertEqual(session.filters.offered, [4])
        guard case .done = session.state else {
            return XCTFail("The current callback was not accepted")
        }
    }

    func testSwitchAwayAndBackCannotReviveOldActionsOrCompletion() throws {
        let session = SearchSession()
        let selected = candidate()
        session.open(selected, peerId: "peer")
        let owner = session.owner
        let run = try XCTUnwrap(session.begin(owner: owner))
        session.retire()
        XCTAssertNil(session.context(for: selected, peerId: "peer"))
        session.open(selected, peerId: "peer")
        let current = try XCTUnwrap(session.begin(owner: session.owner))
        session.cancel(owner: owner)
        session.complete(outcome(offered: [46]), for: run)
        XCTAssertTrue(session.state.isLoading)
        session.complete(outcome(offered: [4]), for: current)
        XCTAssertEqual(session.filters.offered, [4])
    }

    func testReopeningTheSameSelectionPreservesItsPresentationOwnerAndInput() {
        let session = SearchSession()
        let selected = candidate()
        session.open(selected, peerId: "peer")
        let owner = session.owner
        session.filters.country = "DE"
        session.open(selected, peerId: "peer")
        XCTAssertEqual(session.owner, owner)
        XCTAssertEqual(session.filters.country, "DE")
    }

    private func candidate() -> IdentityCandidate {
        IdentityCandidate(location: .slot(UUID()), state: .ready)
    }

    private func outcome(offered: [UInt32]) -> SearchOutcome {
        SearchOutcome(
            searcher: nil,
            result: .success(
                ProfileSearchResult(offered: offered, profiles: [], partial: false, failures: [])))
    }
}
