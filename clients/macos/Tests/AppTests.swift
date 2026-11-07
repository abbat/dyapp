@testable import DYApp
import XCTest

final class AppTests: XCTestCase {
    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.macos")
        XCTAssertEqual(
            Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
            "0.0.1")
    }

    func testStorageNumbersAreMeasuredOrUnknown() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(
            UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        XCTAssertNil(StorageUsage.messageBytes(in: folder))
        try Data(count: 10).write(to: folder.appendingPathComponent("client.db"))
        XCTAssertEqual(StorageUsage.messageBytes(in: folder), 10)
        try Data(count: 5).write(to: folder.appendingPathComponent("client.db-wal"))
        XCTAssertEqual(StorageUsage.messageBytes(in: folder), 15)
        XCTAssertNotNil(StorageUsage.freeBytes(on: folder))
        XCTAssertNil(StorageUsage.freeBytes(on: folder.appendingPathComponent("gone")))
    }

    private func row(
        _ peer: String, state: ChatState = .open, unread: UInt64 = 0, muted: Bool = false,
        text: String? = "hi", isUnread: Bool = true
    ) -> ChatRow {
        let message = ChatMessage(
            groupId: Data([9]), author: peer, id: Data([1]), text: text, edited: false,
            deleted: false,
            unread: isUnread, forwarded: false, delivery: nil, read: false, sentAtMs: nil
        )
        return ChatRow(
            peerId: peer, state: state, lastMessage: message, unreadCount: unread, muted: muted)
    }

    func testNoticeCountLeavesMutedPeersOutAndCountsLikesAndRequests() {
        let rows = [
            row("a", unread: 2), row("b", unread: 5, muted: true),
            row("c", state: .requestReceived),
        ]
        XCTAssertEqual(MailNotice.count(rows: rows, liked: ["b", "d"]), 4)
    }

    func testNoticeLinesFollowTheSettingAndOnlyAnOpenChat() {
        let name: (String) -> String = { "name \($0)" }
        let after = [
            row("muted", unread: 9, muted: true), row("old", unread: 1), row("new", unread: 2),
        ]
        let before = [row("muted"), row("old", unread: 1), row("new", unread: 1)]
        XCTAssertNil(MailNotice.lines(.fixed, before: before, after: after, name: name))
        XCTAssertEqual(
            MailNotice.lines(.name, before: before, after: after, name: name),
            MailNotice.Lines(sender: "name new", text: nil))
        XCTAssertEqual(
            MailNotice.lines(.text, before: before, after: after, name: name),
            MailNotice.Lines(sender: "name new", text: "hi"))
        let request = [row("stranger", state: .requestReceived, unread: 1)]
        XCTAssertNil(MailNotice.lines(.text, before: [], after: request, name: name))
        XCTAssertNil(MailNotice.lines(.text, before: after, after: after, name: name))
    }

    func testNoticeTextIsOneShortLineOfUnreadText() throws {
        let name: (String) -> String = { $0 }
        let long = String(repeating: "\u{1F600}", count: 300)
        let shown = try XCTUnwrap(
            MailNotice.lines(
                .text, before: [], after: [row("a", unread: 1, text: long)], name: name)?.text)
        XCTAssertEqual(shown.unicodeScalars.count, 200)
        let lines = MailNotice.lines(
            .text, before: [], after: [row("a", unread: 1, text: "one\ntwo")], name: name)
        XCTAssertEqual(lines?.text, "one two")
        let read = MailNotice.lines(
            .text, before: [], after: [row("a", unread: 1, isUnread: false)], name: name)
        XCTAssertEqual(read, MailNotice.Lines(sender: "a", text: nil))
        let blank = MailNotice.lines(
            .text, before: [], after: [row("a", unread: 1, text: "\n")], name: name)
        XCTAssertEqual(blank, MailNotice.Lines(sender: "a", text: nil))
    }

    func testSafetyCodeIsASquareImageOfThePayload() throws {
        let code = try XCTUnwrap(safetyQRImage(Data(repeating: 1, count: 65)))
        XCTAssertEqual(code.width, code.height)
        XCTAssertGreaterThan(code.width, 100)
    }

    func testStorageSizesUseBinaryUnits() {
        let english = Locale(identifier: "en_US")
        XCTAssertTrue(StorageUsage.sizeText(0, locale: english).hasPrefix("0 "))
        XCTAssertTrue(StorageUsage.sizeText(1_023, locale: english).hasPrefix("1,023 "))
        XCTAssertTrue(StorageUsage.sizeText(1_536, locale: english).hasPrefix("1.5 "))
        XCTAssertTrue(StorageUsage.sizeText(1_048_576, locale: english).hasPrefix("1.0 "))
    }

    func testStatusBarNamesMeasuredBytesOrSaysUnavailable() throws {
        XCTAssertEqual(
            StatusBar.trafficText(nil), NSLocalizedString("statusbar.noTraffic", comment: ""))
        XCTAssertEqual(
            StatusBar.messagesText(nil), NSLocalizedString("statusbar.noMessages", comment: ""))
        let traffic = StatusBar.trafficText(ClientTraffic(received: 1_536, sent: 3_072))
        XCTAssertLessThan(
            try XCTUnwrap(traffic.range(of: StorageUsage.sizeText(1_536))).lowerBound,
            try XCTUnwrap(traffic.range(of: StorageUsage.sizeText(3_072))).lowerBound
        )
        XCTAssertTrue(StatusBar.messagesText(2_048).contains(StorageUsage.sizeText(2_048)))
    }

    func testASavedFrameStaysOnlyWhereItCanBeReached() {
        let primary = CGRect(x: 0, y: 0, width: 1_000, height: 800)
        let right = CGRect(x: 1_000, y: 0, width: 500, height: 800)
        let onBoth = CGRect(x: 800, y: 100, width: 400, height: 500)
        XCTAssertEqual(WindowPlacement.restored(onBoth, on: [primary, right]), onBoth)
        // A display went away: less than a quarter of the window is left.
        XCTAssertEqual(
            WindowPlacement.restored(
                CGRect(x: 950, y: 100, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        // Most of the window shows, but its title bar is above the display.
        XCTAssertEqual(
            WindowPlacement.restored(
                CGRect(x: 100, y: 400, width: 400, height: 500), on: [primary]),
            CGRect(x: 300, y: 150, width: 400, height: 500)
        )
        XCTAssertEqual(
            WindowPlacement.restored(
                CGRect(x: 5_000, y: 0, width: 2_000, height: 1_000), on: [primary]),
            primary
        )
    }
}
