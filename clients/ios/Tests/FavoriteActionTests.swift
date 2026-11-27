@testable import DYApp
import XCTest

final class FavoriteActionTests: XCTestCase {
    func testDuplicateAdmissionAndLateReadCannotReplaceAChangedFavorite() throws {
        var state = FavoriteActionState()
        let initial = try XCTUnwrap(state.begin(change: false))
        XCTAssertNil(initial.target)
        XCTAssertNil(state.begin(change: true))
        state.complete(.success(false), for: initial)
        let save = try XCTUnwrap(state.begin(change: true))
        XCTAssertTrue(try XCTUnwrap(save.target))
        state.complete(.success(false), for: initial)
        XCTAssertTrue(state.busy)
        state.complete(.success(true), for: save)
        XCTAssertTrue(try XCTUnwrap(state.saved))
        XCTAssertEqual(state.status, "favorites.saved")
        XCTAssertEqual(state.label, "favorites.remove")
    }

    func testFailedReadRetriesReadingAndBlockedSaveKeepsTheKnownFlag() throws {
        var state = FavoriteActionState()
        let initial = try XCTUnwrap(state.begin(change: false))
        state.complete(.failure(CoreError.Failed(message: "unavailable")), for: initial)
        XCTAssertNil(state.saved)
        XCTAssertEqual(state.label, "favorites.retry")
        let retry = try XCTUnwrap(state.begin(change: true))
        XCTAssertNil(retry.target, "An unknown saved flag must be read, never guessed or toggled")
        state.complete(.success(false), for: retry)
        let save = try XCTUnwrap(state.begin(change: true))
        state.complete(.failure(FavoriteError.Blocked), for: save)
        XCTAssertFalse(try XCTUnwrap(state.saved))
        XCTAssertEqual(state.status, "favorites.blocked")
        XCTAssertFalse(state.busy)
    }

    func testLateTicketCannotCompleteAnIndependentPresentation() throws {
        var previous = FavoriteActionState()
        let old = try XCTUnwrap(previous.begin(change: false))
        var successor = FavoriteActionState()
        let current = try XCTUnwrap(successor.begin(change: false))
        successor.complete(.success(true), for: old)
        XCTAssertTrue(successor.busy)
        XCTAssertNil(successor.saved)
        successor.complete(.success(true), for: current)
        let remove = try XCTUnwrap(successor.begin(change: true))
        XCTAssertFalse(try XCTUnwrap(remove.target))
        successor.complete(.success(false), for: remove)
        XCTAssertEqual(successor.status, "favorites.removed")
    }
}
