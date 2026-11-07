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
        XCTAssertTrue(chatSendable(.open, "hi"))
        XCTAssertFalse(chatSendable(.open, " \n"))
        XCTAssertTrue(chatSendable(.open, String(repeating: "a", count: 65_536)))
        XCTAssertFalse(chatSendable(.open, String(repeating: "a", count: 65_537)))
        XCTAssertTrue(
            chatTooLong(String(repeating: "\u{44f}", count: 32_769)), "the limit counts bytes")
    }

    func testAFailedSendThatStoredItsTextReadsAsQueued() {
        let own = message(author: "me", id: 1)
        XCTAssertTrue(stagedNew(before: [], after: [own], me: "me"))
        XCTAssertFalse(stagedNew(before: [own], after: [own], me: "me"))
        XCTAssertFalse(stagedNew(before: [], after: [message(author: "peer", id: 2)], me: "me"))

        let offline = CoreError.Failed(message: "notConnected")
        XCTAssertEqual(
            chatSendStatus(offline, staged: true), ChatStatus(key: "chat.queued", details: nil))
        XCTAssertEqual(chatSendStatus(offline, staged: false).key, "search.notConnected")
        XCTAssertNotNil(chatSendStatus(offline, staged: false).details, "the draft stays")
        XCTAssertEqual(
            chatSendStatus(CoreError.Failed(message: "unpublished"), staged: false).key,
            "chat.unpublished")
        XCTAssertEqual(
            chatSendStatus(CoreError.Failed(message: "other"), staged: false).key, "chat.sendFailed"
        )
        let edit = {
            chatSendStatus(
                offline, staged: $0, queued: "chat.editQueued", failed: "chat.editFailed")
        }
        XCTAssertEqual(edit(true), ChatStatus(key: "chat.editQueued", details: nil))
        XCTAssertEqual(edit(false).key, "search.notConnected", "as a send, no connection is named")
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
