@testable import DYApp
import UserNotifications
import XCTest

final class AppTests: XCTestCase {
    func testBackgroundReceiveIsPermitted() {
        let info = Bundle.main.object(forInfoDictionaryKey: "BGTaskSchedulerPermittedIdentifiers") as? [String]
        XCTAssertEqual(info, [MailJob.identifier])
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String], ["fetch"])
    }

    func testNoticeAddsLinesOnlyWhileTheLockScreenHidesThem() {
        let lines = MailNotice.Lines(sender: "name", text: "hi")
        XCTAssertNil(MailJob.shown(lines, previews: .always))
        XCTAssertEqual(MailJob.shown(lines, previews: .whenAuthenticated), lines)
        XCTAssertEqual(MailJob.shown(lines, previews: .never), lines)
    }

    func testNoticeHoldsTheFixedTextWithoutLines() {
        let fixed = NSLocalizedString("notice.newMessage", comment: "")
        XCTAssertEqual(MailNotice.request(nil).content.title, fixed)
        XCTAssertEqual(MailNotice.request(nil).content.body, "")
        let named = MailNotice.request(MailNotice.Lines(sender: "name", text: nil)).content
        XCTAssertEqual(named.title, "name")
        XCTAssertEqual(named.body, fixed)
        XCTAssertEqual(MailNotice.request(nil).identifier, MailNotice.request(MailNotice.Lines(sender: "name", text: "hi")).identifier)
    }

    func testBuiltApplicationIdentity() {
        XCTAssertEqual(Bundle.main.bundleIdentifier, "com.dyapp.ios")
        XCTAssertEqual(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String, "0.0.1")
    }
}
