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

    func testCoreBoundedTextIsRenderedVerbatim() {
        let text = "First line\nSecond line\u{2026}"
        let preview = ChatListPreview(body: .text(text: text), forwarded: false, sentAtMs: nil)
        XCTAssertEqual(chatListPreview(preview), text)
    }
}
