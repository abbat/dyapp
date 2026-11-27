@testable import DYApp
import UserNotifications
import XCTest

final class MailNoticeAdapterTests: XCTestCase {
    func testNoNoticeOrMissingAuthorizationNeverResolvesPrivateNamesOrPostsFixedText() {
        let notice = ChatMailNotice(peerId: "peer", text: "private")
        for authorization: UNAuthorizationStatus in [
            .notDetermined, .denied, .provisional, .ephemeral,
        ] {
            XCTAssertNil(
                MailNotice.request(
                    notice, authorization: authorization, previews: .whenAuthenticated
                ) { _ in
                    XCTFail("No authorization must mean no private resolver"); return "private"
                })
        }
        XCTAssertNil(
            MailNotice.request(
                nil, authorization: .authorized, previews: .whenAuthenticated
            ) { _ in
                XCTFail("No core notice is quiet, not a fixed notice"); return "private"
            })
    }

    func testFixedNoticeNeverResolvesAPeerOrUsesUnadmittedText() throws {
        let notice = ChatMailNotice(peerId: nil, text: "must not leak")
        let request = try XCTUnwrap(
            MailNotice.request(
                notice, authorization: .authorized, previews: .never
            ) { _ in
                XCTFail("Fixed mode has no peer selector"); return "private"
            })
        XCTAssertEqual(request.content.title, NSLocalizedString("notice.newMessage", comment: ""))
        XCTAssertEqual(request.content.body, "")
    }

    func testCoreTextIncludingStickerAndMarkupIsPresentedLiterally() throws {
        for text in ["<b>literal & text</b>", "\u{1f63a} Sticker"] {
            let request = try XCTUnwrap(
                MailNotice.request(
                    ChatMailNotice(peerId: "peer", text: text), authorization: .authorized,
                    previews: .whenAuthenticated
                ) { peer in
                    XCTAssertEqual(peer, "peer")
                    return "local\nname"
                })
            XCTAssertEqual(request.content.title, "local name")
            XCTAssertEqual(request.content.body, text)
        }
    }

    func testNameOnlyAndBlankBodiesUseFixedCopyWhileTheNativeLabelKeepsItsScalarBound() throws {
        let name = String(repeating: "\u{1f600}", count: 205)
        for text: String? in [nil, " \t "] {
            let request = try XCTUnwrap(
                MailNotice.request(
                    ChatMailNotice(peerId: "peer", text: text), authorization: .authorized,
                    previews: .whenAuthenticated
                ) { _ in name })
            XCTAssertEqual(request.content.title, String(repeating: "\u{1f600}", count: 200))
            XCTAssertEqual(
                request.content.body, NSLocalizedString("notice.newMessage", comment: ""))
        }
    }
}
