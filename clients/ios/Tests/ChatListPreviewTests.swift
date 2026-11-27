@testable import DYApp
import XCTest

final class ChatListPreviewTests: XCTestCase {
    func testTypedMediaDoesNotFallBackToNoMessages() {
        let photo = ChatListPreview(body: .photo, forwarded: true, sentAtMs: 42)
        let photoLabel = NSLocalizedString("chat.photo", comment: "")
        XCTAssertEqual(
            chatListPreview(photo),
            String(format: NSLocalizedString("chats.forwarded", comment: ""), photoLabel))
        let sticker = ChatListPreview(
            body: .sticker(emoji: "\u{1f63a}", packName: "Cats"), forwarded: false, sentAtMs: nil)
        XCTAssertEqual(chatListPreview(sticker), "\u{1f63a} Cats")
    }

    func testEmptyAndDeletedPreviewsHaveDistinctLabels() {
        let empty = ChatListPreview(body: .empty, forwarded: false, sentAtMs: nil)
        let deleted = ChatListPreview(body: .deleted, forwarded: false, sentAtMs: 42)
        XCTAssertEqual(chatListPreview(empty), NSLocalizedString("chats.noMessages", comment: ""))
        XCTAssertEqual(chatListPreview(deleted), NSLocalizedString("chats.deleted", comment: ""))
    }

    func testProfilePreviewShowsOnlyTheShortPeerID() {
        let peer = String(repeating: "a", count: 58) + "123456"
        let preview = ChatListPreview(body: .profile(peerId: peer), forwarded: false, sentAtMs: nil)
        let expected = String(
            format: NSLocalizedString("chats.profileLink", comment: ""), "@123456")
        XCTAssertEqual(chatListPreview(preview), expected)
    }

    func testCoreBoundedTextIsRenderedVerbatim() {
        let text = "First line\nSecond line\u{2026}"
        let preview = ChatListPreview(body: .text(text: text), forwarded: false, sentAtMs: nil)
        XCTAssertEqual(chatListPreview(preview), text)
    }
}
