@testable import DYApp
import UserNotifications
import XCTest

final class AppTests: XCTestCase {
    func testEarlierSearchDeadlineAdvancesMailRefresh() async {
        let now = Date(timeIntervalSince1970: 1_000)
        let deadline = now.addingTimeInterval(42)
        let requests = RefreshRequests()
        let refresh = MailRefresh(poll: { deadline }, submit: requests.append)
        await refresh.schedule(now: now)
        await refresh.wake()
        await refresh.schedule(now: now)
        XCTAssertEqual(requests.dates, [now.addingTimeInterval(MailRefresh.period), deadline])
    }

    func testBackgroundReceiveIsPermitted() {
        let info =
            Bundle.main.object(forInfoDictionaryKey: "BGTaskSchedulerPermittedIdentifiers")
            as? [String]
        XCTAssertEqual(info, [MailJob.identifier])
        XCTAssertEqual(
            Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String], ["fetch"])
    }

    func testNoticeAddsLinesOnlyWhileTheLockScreenHidesThem() {
        let notice = ChatMailNotice(peerId: "peer", text: "hi")
        let fixed = NSLocalizedString("notice.newMessage", comment: "")
        let hidden = MailNotice.request(notice, authorization: .authorized, previews: .always) {
            _ in
            XCTFail("Always-visible previews must not resolve private names")
            return "name"
        }
        XCTAssertEqual(hidden?.content.title, fixed)
        XCTAssertEqual(hidden?.content.body, "")
        for previews: UNShowPreviewsSetting in [.whenAuthenticated, .never] {
            let shown = MailNotice.request(notice, authorization: .authorized, previews: previews) {
                _ in "name"
            }
            XCTAssertEqual(shown?.content.title, "name")
            XCTAssertEqual(shown?.content.body, "hi")
        }
    }

    func testNoticeHoldsTheFixedTextWithoutLines() {
        let fixed = NSLocalizedString("notice.newMessage", comment: "")
        XCTAssertEqual(MailNotice.request(nil).content.title, fixed)
        XCTAssertEqual(MailNotice.request(nil).content.body, "")
        let named = MailNotice.request(MailNotice.Lines(sender: "name", text: nil)).content
        XCTAssertEqual(named.title, "name")
        XCTAssertEqual(named.body, fixed)
        XCTAssertEqual(
            MailNotice.request(nil).identifier,
            MailNotice.request(MailNotice.Lines(sender: "name", text: "hi")).identifier)
    }

    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.ios")
        XCTAssertEqual(
            Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String,
            "0.0.1")
    }
}

private final class RefreshRequests: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Date] = []

    var dates: [Date] { lock.withLock { values } }

    func append(_ date: Date) { lock.withLock { values.append(date) } }
}
