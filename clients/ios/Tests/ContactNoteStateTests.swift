@testable import DYApp
import XCTest

final class ContactNoteStateTests: XCTestCase {
    func testUnknownAndFailedReadCannotBecomeAnEditableEmptyNote() throws {
        var state = ContactNoteState()
        XCTAssertFalse(state.editable)
        XCTAssertNil(state.begin(.save))
        let read = try XCTUnwrap(state.begin(.read))
        XCTAssertNil(state.begin(.read))
        state.complete(.failure(ContactNoteError.Failed(reason: "storage")), for: read)
        XCTAssertNil(state.saved)
        XCTAssertEqual(state.status, "notes.loadFailed")
        XCTAssertFalse(state.editable)
        let retry = try XCTUnwrap(state.begin(.read))
        state.complete(.success(""), for: retry)
        XCTAssertTrue(state.editable)
        XCTAssertEqual(state.saved, "")
        XCTAssertFalse(state.canSave)
        XCTAssertFalse(state.canClear)
    }

    func testFailedSaveAndClearKeepTextAndSubmittedSnapshotUntilExplicitRetry() throws {
        var state = ContactNoteState()
        let read = try XCTUnwrap(state.begin(.read))
        state.complete(.success(""), for: read)
        let text = "  Private note\nMixed CASE \u{1f600}  "
        state.edit(text)
        let save = try XCTUnwrap(state.begin(.save))
        XCTAssertEqual(save.target, text)
        state.edit("An edit while disabled")
        XCTAssertEqual(state.draft, text)
        state.complete(.failure(ContactNoteError.InvalidText), for: save)
        XCTAssertEqual(state.draft, text)
        XCTAssertEqual(state.saved, "")
        XCTAssertEqual(state.status, "notes.invalid")
        XCTAssertTrue(state.canSave)
        let retry = try XCTUnwrap(state.begin(.save))
        state.complete(.success(text), for: retry)
        XCTAssertEqual(state.status, "notes.saved")
        let clear = try XCTUnwrap(state.begin(.clear))
        XCTAssertEqual(clear.target, "")
        state.complete(.failure(ContactNoteError.Failed(reason: "storage")), for: clear)
        XCTAssertEqual(state.saved, text)
        XCTAssertEqual(state.draft, text)
        XCTAssertEqual(state.status, "notes.saveFailed")
        let retryClear = try XCTUnwrap(state.begin(.clear))
        state.complete(.success(""), for: retryClear)
        XCTAssertEqual(state.status, "notes.cleared")
        XCTAssertEqual(state.draft, "")
    }

    func testAnObsoleteReadCannotOverwriteANewDraftOrAnotherOpening() throws {
        var first = ContactNoteState()
        let old = try XCTUnwrap(first.begin(.read))
        first.complete(.success("saved"), for: old)
        first.edit("unsaved")
        let save = try XCTUnwrap(first.begin(.save))
        first.complete(.success("old read"), for: old)
        XCTAssertEqual(first.draft, "unsaved")
        XCTAssertTrue(first.busy)
        var next = ContactNoteState()
        let current = try XCTUnwrap(next.begin(.read))
        next.complete(.success("old saved value"), for: save)
        XCTAssertNil(next.saved)
        XCTAssertTrue(next.busy)
        next.complete(.success("current"), for: current)
        XCTAssertEqual(next.draft, "current")
    }
}
