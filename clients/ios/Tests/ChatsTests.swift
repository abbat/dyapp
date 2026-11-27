@testable import DYApp
import XCTest

final class ChatsTests: XCTestCase {
    func testNativeChatBootstrapNeverReplacesTheSavedWrappedKeys() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let directory = root.appendingPathComponent("profiles")
            .appendingPathComponent(created.slot.id.uuidString.lowercased())
        try openChats(store, created.slot.id, peer: created.peerId)
        let device = try Data(contentsOf: directory.appendingPathComponent("device.key"))
        let state = try Data(contentsOf: directory.appendingPathComponent("chat-state.key"))
        try openChats(store, created.slot.id, peer: created.peerId)
        XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent("device.key")), device)
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("chat-state.key")), state)
        XCTAssertEqual(try store.open(created.slot).peerId(), created.peerId)
        let corrupt = Data("invalid existing device key".utf8)
        try corrupt.write(to: directory.appendingPathComponent("device.key"), options: .atomic)
        XCTAssertThrowsError(try openChats(store, created.slot.id, peer: created.peerId))
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("device.key")), corrupt)
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("chat-state.key")), state)
    }

    func testNativeChatBootstrapRefusesWrongOwnerAndMissingWrappingStoreBeforeWrites() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let id = created.slot.id.uuidString.lowercased()
        let directory = root.appendingPathComponent("profiles").appendingPathComponent(id)
        let owner = try Data(contentsOf: directory.appendingPathComponent("identity.key"))
        let absent = PlatformKeyStore.forSlot(UUID())
        XCTAssertFalse(try absent.containsWrappingKey())
        XCTAssertThrowsError(
            try store.registry.openChats(
                id: id, expectedPeer: created.peerId, store: absent.requiringExistingKey()))
        XCTAssertFalse(try absent.containsWrappingKey())
        XCTAssertThrowsError(try openChats(store, created.slot.id, peer: "wrong owner"))
        for name in ["device.key", "chat-state.key", "client.db"] {
            XCTAssertFalse(
                FileManager.default.fileExists(atPath: directory.appendingPathComponent(name).path))
        }
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("identity.key")), owner)
        XCTAssertEqual(try store.open(created.slot).peerId(), created.peerId)
    }

    private func openChats(_ store: IdentitySlotStore, _ id: UUID, peer: String) throws {
        let session = try store.registry.openChats(
            id: id.uuidString.lowercased(), expectedPeer: peer,
            store: PlatformKeyStore.forSlot(id).requiringExistingKey())
        XCTAssertTrue(try session.chatRows().isEmpty)
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

    func testForwardedTextUsesTheLocalizedMarkerForEitherSender() {
        for author in ["me", "peer"] {
            var forwarded = message(author: author, id: 1)
            forwarded.forwarded = true
            let expected = String(
                format: NSLocalizedString("chats.forwarded", comment: ""), forwarded.text)
            XCTAssertEqual(chatPreview(forwarded), expected)
            XCTAssertEqual(forwarded.author, author)
        }
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
