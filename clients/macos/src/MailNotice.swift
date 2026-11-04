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

/// The new-mail notice, as on the Linux and Android clients.
enum MailNotice {
    /// The sender and, when the setting allows it, the message a notice adds to the fixed text.
    struct Lines: Equatable {
        let sender: String
        let text: String?
    }

    /// One id, so each notice replaces the previous one.
    private static let requestId = "mail"
    /// The most of a message a notice shows; the rest waits in the app.
    private static let textLimit = 200
    private static let clicks = NoticeClicks()
    @MainActor static var showWindow: (() -> Void)?

    /// Unread items and unanswered likes, muted peers left out. A received request counts even when read.
    static func count(rows: [ChatRow], liked: [String]) -> UInt64 {
        let unmuted = rows.filter { !$0.muted }
        let muted = Set(rows.filter(\.muted).map(\.peerId))
        let unread = unmuted.reduce(UInt64(0)) { $0 + ($1.state == .requestReceived ? max($1.unreadCount, 1) : $1.unreadCount) }
        return unread + UInt64(liked.filter { !muted.contains($0) }.count)
    }

    /// What `notice` allows for the newest unmuted row with more unread than in `before`; nil keeps the
    /// fixed text. Only an open chat adds lines: a request or a like would show a stranger's words unasked.
    static func lines(
        _ notice: ChatNotice, before: [ChatRow], after: [ChatRow], name: (String) -> String
    ) -> Lines? {
        guard notice == .name || notice == .text else { return nil }
        let grown = after.first { row in
            !row.muted && row.unreadCount > (before.first { $0.peerId == row.peerId }?.unreadCount ?? 0)
        }
        guard let row = grown, row.state == .open else { return nil }
        // shortcut: the FFI row has no attachment, so a newer unread attachment shows the last unread text.
        let text = notice == .text && row.lastMessage?.unread == true ? row.lastMessage?.text : nil
        let line = text.map(oneLine)?.trimmingCharacters(in: .whitespaces)
        return Lines(sender: oneLine(name(row.peerId)), text: line?.isEmpty == false ? line : nil)
    }

    /// At most `textLimit` code points on one line: a peer chooses this text.
    private static func oneLine(_ text: String) -> String {
        var line = String.UnicodeScalarView()
        for scalar in text.unicodeScalars.prefix(textLimit) {
            line.append(CharacterSet.controlCharacters.contains(scalar) ? " " : scalar)
        }
        return String(line)
    }

    static func status() async -> UNAuthorizationStatus {
        await withCheckedContinuation { done in
            UNUserNotificationCenter.current().getNotificationSettings { done.resume(returning: $0.authorizationStatus) }
        }
    }

    /// A refusal is only quiet here: the settings section reads the status and says so.
    static func request() async {
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in done.resume() }
        }
    }

    /// Shows the notice unless the window has the focus or macOS blocks notices.
    @MainActor
    static func post(_ lines: Lines?) async {
        if NSApp.isActive, NSApp.keyWindow != nil { return }
        guard await status() == .authorized else { return }
        let center = UNUserNotificationCenter.current()
        // shortcut: set at the first notice to keep the notification service off the launch path, so a
        // notice left by an earlier run only activates the app; set it at launch if that matters.
        center.delegate = clicks
        let fixed = NSLocalizedString("notice.newMessage", comment: "")
        let content = UNMutableNotificationContent()
        content.title = lines?.sender ?? fixed
        if let lines { content.body = lines.text ?? fixed }
        center.add(UNNotificationRequest(identifier: requestId, content: content, trigger: nil), withCompletionHandler: nil)
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

/// The per-identity notice setting. A failed read or save keeps the last value and says so.
struct MailNoticeSection: View {
    @ObservedObject var feed: ChatFeed
    /// Nil until read: the controls stay disabled.
    @State private var on: Bool?
    /// Kept while off, so a turn-on brings the earlier choice back.
    @State private var content = ChatNotice.fixed
    @State private var failed = false
    @State private var allowed = true
    /// macOS shows its question once; after that Allow opens System Settings.
    @State private var asked = false

    var body: some View {
        Section {
            Toggle("notice.toggle", isOn: Binding(get: { on ?? false }, set: { save($0, content) }))
                .disabled(on == nil)
                .accessibilityIdentifier("message-notifications")
                .task(id: feed.me) { await load() }
            Picker("notice.content", selection: Binding(get: { content }, set: { save(true, $0) })) {
                Text("notice.contentFixed").tag(ChatNotice.fixed)
                Text("notice.contentName").tag(ChatNotice.name)
                Text("notice.contentText").tag(ChatNotice.text)
            }
            .pickerStyle(.radioGroup)
            .disabled(on != true)
            .accessibilityIdentifier("notice-content")
            Text("notice.hint").font(.caption)
            if failed {
                Text("notice.failed").font(.caption).accessibilityIdentifier("notice-error")
            }
            if on == true, !allowed {
                Text("notice.blocked").font(.caption).accessibilityIdentifier("notice-blocked")
                Button("notice.allow") { Task { await allow() } }.accessibilityIdentifier("notice-allow")
            }
        } header: {
            Text("notice.title")
        }
        // The user comes back from System Settings with the answer.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await refreshAllowed() }
        }
    }

    private func refreshAllowed() async {
        allowed = await MailNotice.status() == .authorized
    }

    private func load() async {
        await refreshAllowed()
        guard feed.me != nil else { return }
        switch await feed.call({ (try $0.notice(), try $0.noticeContent()) }) {
        case let .success((notice, stored)):
            on = notice != .off
            content = stored
            failed = false

        case .failure:
            failed = true
        }
    }

    private func save(_ turnedOn: Bool, _ chosen: ChatNotice) {
        let notice = turnedOn ? chosen : ChatNotice.off
        Task {
            guard case .success = await feed.call({ try $0.setNotice(notice: notice) }) else {
                failed = true
                return
            }
            failed = false
            on = turnedOn
            content = chosen
            if turnedOn, !allowed { await allow() }
        }
    }

    private func allow() async {
        if !asked, await MailNotice.status() == .notDetermined {
            asked = true
            await MailNotice.request()
        } else if let settings = URL(string: "x-apple.systempreferences:com.apple.preference.notifications") {
            NSWorkspace.shared.open(settings)
        }
        await refreshAllowed()
    }
}
