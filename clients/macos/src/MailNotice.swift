import AppKit
import SwiftUI
@preconcurrency import UserNotifications

/// Shows the window when the user clicks a notice; the system center keeps its delegate weakly.
private final class NoticeClicks: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        completionHandler()
        Task { @MainActor in MailNotice.showWindow?() }
    }
}

extension MailNotice {
    private static let clicks = NoticeClicks()
    @MainActor static var showWindow: (() -> Void)?

    /// Shows the notice unless the window has the focus or macOS blocks notices.
    @MainActor
    static func post(_ lines: Lines?) async {
        if NSApp.isActive, NSApp.keyWindow != nil { return }
        guard await status() == .authorized else { return }
        let center = UNUserNotificationCenter.current()
        // shortcut: set at the first notice to keep the notification service off the launch path, so a
        // notice left by an earlier run only activates the app; set it at launch if that matters.
        center.delegate = clicks
        center.add(request(lines), withCompletionHandler: nil)
    }
}

extension ChatFeed {
    /// Notices what the pass that started from `before` and `likedBefore` brought. A failed read of the
    /// setting stays quiet, as it may be off.
    func noticeNewMail(before: [ChatRow], likedBefore: [String]) async {
        guard MailNotice.count(rows: rows, liked: liked) > MailNotice.count(rows: before, liked: likedBefore),
              case let .success(notice) = await call({ try $0.notice() }), notice != .off else { return }
        await MailNotice.post(MailNotice.lines(notice, before: before, after: rows) { contactLabel($0, names[$0]) })
    }
}
