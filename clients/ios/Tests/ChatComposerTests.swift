@testable import DYApp
import XCTest

@MainActor
final class ChatComposerTests: XCTestCase {
    private func message(_ id: UInt8, group: UInt8 = 9, deleted: Bool = false) -> ChatMessage {
        ChatMessage(
            groupId: Data([group]), author: "owner", id: Data([id]), text: "original",
            edited: false, deleted: deleted, unread: false, forwarded: false, delivery: nil,
            read: false, sentAtMs: nil)
    }

    func testCommittedSendConsumesOnlyTheExactDraftRevision() throws {
        for changed in [false, true] {
            let composer = ChatComposer()
            composer.draft = "submitted"
            let pending = try XCTUnwrap(composer.begin(state: .open, messages: []))
            if changed {
                composer.draft = "new text"
                composer.draft = "submitted"
            }
            composer.complete(pending, result: .success(.accepted(id: Data([1]))))
            XCTAssertEqual(composer.draft, changed ? "submitted" : "")
            XCTAssertFalse(composer.sending)
        }
    }

    func testQueuedEditRestoresHeldDraftOnlyWhenNothingChanged() throws {
        for changed in [false, true] {
            let composer = ChatComposer()
            composer.draft = "held"
            composer.edit(message(1))
            composer.draft = "submitted edit"
            let pending = try XCTUnwrap(composer.begin(state: .open, messages: [message(1)]))
            if changed { composer.draft = "newer edit" }
            composer.complete(
                pending, result: .success(.queued(id: Data([1]), detail: "offline")))
            XCTAssertEqual(composer.draft, changed ? "newer edit" : "held")
            XCTAssertEqual(composer.editing, changed ? message(1).threadKey : nil)
            XCTAssertEqual(composer.status, ChatStatus(key: "chat.editQueued", details: nil))
        }
    }

    func testPrecommitFailureKeepsTheEditAndHeldDraft() throws {
        let composer = ChatComposer()
        composer.draft = "held"
        composer.edit(message(1))
        composer.draft = "edit"
        let pending = try XCTUnwrap(composer.begin(state: .open, messages: [message(1)]))
        composer.complete(pending, result: .failure(CoreError.Failed(message: "refused")))
        XCTAssertEqual(composer.draft, "edit")
        XCTAssertEqual(composer.editing, message(1).threadKey)
        XCTAssertNotNil(composer.status?.details)
        composer.endEditing()
        XCTAssertEqual(composer.draft, "held")
    }

    func testCancelledAndSuccessorEditCannotBeConsumedByPriorCompletion() throws {
        let composer = ChatComposer()
        composer.draft = "held"
        composer.edit(message(1))
        let pending = try XCTUnwrap(composer.begin(state: .open, messages: [message(1)]))
        composer.endEditing()
        composer.edit(message(2))
        composer.complete(pending, result: .success(.accepted(id: Data([1]))))
        XCTAssertEqual(composer.editing, message(2).threadKey)
        XCTAssertEqual(composer.draft, "original")
        XCTAssertNil(composer.status)
        composer.endEditing()
        XCTAssertEqual(composer.draft, "held")
    }

    func testOldOpeningCannotUnlockOrConsumeASuccessorSubmission() throws {
        let composer = ChatComposer()
        composer.draft = "same text"
        let old = try XCTUnwrap(composer.begin(state: .open, messages: []))
        composer.retire()
        let current = try XCTUnwrap(composer.begin(state: .open, messages: []))
        composer.complete(old, result: .success(.accepted(id: Data([1]))))
        XCTAssertTrue(composer.sending)
        XCTAssertEqual(composer.draft, "same text")
        composer.complete(current, result: .success(.accepted(id: Data([2]))))
        XCTAssertFalse(composer.sending)
        XCTAssertEqual(composer.draft, "")
    }

    func testDeleteOrClearCompletionCannotRestoreOverNewerEditing() {
        let composer = ChatComposer()
        composer.draft = "held"
        composer.edit(message(1))
        let deletedEdit = composer.edit
        composer.draft = "newer typing"
        composer.endEditing(ifUnchanged: deletedEdit)
        XCTAssertEqual(composer.draft, "newer typing")
        composer.edit(message(2))
        composer.endEditing(ifUnchanged: deletedEdit)
        XCTAssertEqual(composer.editing, message(2).threadKey)
        composer.endEditing(ifUnchanged: composer.edit)
        XCTAssertEqual(composer.draft, "held")
    }

    func testMissingDeletedOrRotatedEditTargetNeverBecomesNewText() {
        for targets in [[], [message(1, deleted: true)], [message(1, group: 8)]] {
            let composer = ChatComposer()
            composer.edit(message(1))
            XCTAssertNil(composer.begin(state: .open, messages: targets))
            XCTAssertFalse(composer.sending)
            XCTAssertEqual(composer.draft, "original")
        }
    }
}
