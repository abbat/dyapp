@testable import DYApp
import XCTest

final class ChatsTests: XCTestCase {
    func testWrappedKeyFileIsMadeOnce() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: file) }
        XCTAssertEqual(try wrappedKeyFile(file) { Data([1]) }, Data([1]))
        XCTAssertEqual(
            try wrappedKeyFile(file) { Data([2]) }, Data([1]), "a saved key is never replaced")
    }

    func testContactLabelPrefersTheLocalNameAndMarksAPublicOne() {
        let view = { (name: String, verified: Bool?) in
            ContactNameView(peerId: "peer123456", name: name, verified: verified, photo: Data())
        }
        XCTAssertEqual(contactLabel("peer123456", nil), "@123456")
        XCTAssertEqual(contactLabel("peer123456", view("", nil)), "@123456")
        XCTAssertEqual(contactLabel("peer123456", view("Bobby", nil)), "Bobby")
        let verified = contactLabel("peer123456", view("Bob", true))
        let unverified = contactLabel("peer123456", view("Bob", false))
        XCTAssertNotEqual(verified, unverified)
        for label in [verified, unverified] {
            XCTAssertTrue(label.contains("Bob") && label.contains("@123456"), label)
        }
    }

    func testWhatAChatTakes() {
        XCTAssertFalse(chatWritable(nil), "still loading")
        XCTAssertFalse(chatWritable(.requestSent))
        XCTAssertFalse(chatWritable(.closed))
        XCTAssertTrue(chatWritable(ChatState.none))
        XCTAssertTrue(chatWritable(.requestReceived))
        XCTAssertTrue(chatWritable(.open))
        for state: ChatState? in [nil, .requestSent, .closed] {
            XCTAssertFalse(chatSendable(state, "hi"))
        }
        for state in [ChatState.none, .requestReceived, .open] {
            XCTAssertTrue(chatSendable(state, "hi"))
        }
        XCTAssertTrue(chatSendable(.open, "hi"))
        XCTAssertFalse(chatSendable(.open, " \n"))
        XCTAssertTrue(chatSendable(.open, String(repeating: "a", count: 65_536)))
        XCTAssertFalse(chatSendable(.open, String(repeating: "a", count: 65_537)))
        XCTAssertTrue(
            chatComposerPolicy(state: .open, text: String(repeating: "\u{44f}", count: 32_769))
                .tooLong,
            "the limit counts bytes")
    }

    func testComposerUsesSharedWhitespaceAndMultibyteBoundaries() {
        // Core uses the same trim rules as Android; Foundation treats BOM and NEL differently.
        XCTAssertFalse(chatSendable(.open, "\u{feff}\u{a0} \n"))
        XCTAssertTrue(chatSendable(.open, "\u{85}"))
        XCTAssertTrue(chatSendable(.open, String(repeating: "\u{44f}", count: 32_768)))
        XCTAssertFalse(chatSendable(.open, String(repeating: "\u{44f}", count: 32_769)))
    }

    func testNativeCommitmentDeterminesAcceptedAndQueuedPresentation() {
        let accepted: Result<ChatSubmission, Error> = .success(.accepted(id: Data([1])))
        let queued: Result<ChatSubmission, Error> = .success(
            .queued(id: Data([1]), detail: "offline after commitment"))
        XCTAssertNil(chatSendStatus(accepted))
        XCTAssertEqual(chatSendStatus(queued), ChatStatus(key: "chat.queued", details: nil))
        for operation in ["edit", "delete"] {
            XCTAssertNil(chatSendStatus(accepted, queued: "chat.\(operation)Queued"))
            XCTAssertEqual(
                chatSendStatus(queued, queued: "chat.\(operation)Queued"),
                ChatStatus(key: "chat.\(operation)Queued", details: nil))
        }
    }

    func testPrecommitFailureRetainsFailureDetails() {
        let offline = CoreError.Failed(message: "notConnected")
        XCTAssertEqual(chatSendStatus(.failure(offline))?.key, "search.notConnected")
        XCTAssertNotNil(chatSendStatus(.failure(offline))?.details, "the draft stays")
        XCTAssertEqual(
            chatSendStatus(.failure(CoreError.Failed(message: "unpublished")))?.key,
            "chat.unpublished")
        for operation in ["send", "edit", "delete"] {
            let failed = "chat.\(operation)Failed"
            let outcome = chatSendStatus(
                .failure(CoreError.Failed(message: "other")), failed: failed)
            XCTAssertEqual(outcome?.key, failed)
            XCTAssertNotNil(outcome?.details)
        }
    }

    func testMetaOfADeletedMessageOmitsItsDelivery() {
        var sent = message(author: "me", id: 1)
        sent.delivery = .pending
        XCTAssertEqual(chatMeta(sent), NSLocalizedString("chat.pending", comment: ""))
        sent.read = true
        XCTAssertEqual(chatMeta(sent), NSLocalizedString("chat.read", comment: ""))
        sent.deleted = true
        XCTAssertEqual(chatMeta(sent), "")
    }

    private func message(author: String, id: UInt8) -> ChatMessage {
        ChatMessage(
            groupId: Data([9]), author: author, id: Data([id]), text: "text", edited: false,
            deleted: false,
            unread: false, forwarded: false, delivery: nil, read: false, sentAtMs: nil
        )
    }

    func testProfileLoadNamesAMissingConnection() {
        XCTAssertEqual(
            profileLoadKey(CoreError.Failed(message: "notConnected")), "search.notConnected")
        XCTAssertEqual(profileLoadKey(CoreError.Failed(message: "other")), "view.loadFailed")
    }

    func testLikeFailureNamesItsCause() {
        XCTAssertEqual(
            likeFailureKey(CoreError.Failed(message: "unpublished")), "view.likeUnpublished")
        XCTAssertEqual(
            likeFailureKey(CoreError.Failed(message: "notConnected")), "view.likeNotConnected")
        XCTAssertEqual(likeFailureKey(CoreError.Failed(message: "other")), "view.likeFailed")
    }
}
