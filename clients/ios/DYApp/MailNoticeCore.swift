import SwiftUI
@preconcurrency import UserNotifications

/// The new-mail notice, as on the Linux and Android clients.
enum MailNotice {
    /// The sender and, when the setting allows it, the message a notice adds to the fixed text.
    struct Lines: Equatable {
        let sender: String
        let text: String?
    }

    /// One id, so each notice replaces the previous one.
    private static let requestId = "mail"

    /// OS gates never change core selection; no notice stays quiet, while no peer means fixed text.
    static func request(
        _ notice: ChatMailNotice?, authorization: UNAuthorizationStatus,
        previews: UNShowPreviewsSetting, name: (String) -> String
    ) -> UNNotificationRequest? {
        guard authorization == .authorized, let notice else { return nil }
        guard previews != .always, let peer = notice.peerId else { return request(nil) }
        let text = notice.text.flatMap { value in
            value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : value
        }
        return request(Lines(sender: mailNoticeLine(text: name(peer)), text: text))
    }

    /// The replacing notice: the fixed text alone, or the lines above it.
    static func request(_ lines: Lines?) -> UNNotificationRequest {
        let fixed = NSLocalizedString("notice.newMessage", comment: "")
        let content = UNMutableNotificationContent()
        content.title = lines?.sender ?? fixed
        if let lines { content.body = lines.text ?? fixed }
        return UNNotificationRequest(identifier: requestId, content: content, trigger: nil)
    }

    static func status() async -> UNAuthorizationStatus {
        await withCheckedContinuation { done in
            UNUserNotificationCenter.current().getNotificationSettings {
                done.resume(returning: $0.authorizationStatus)
            }
        }
    }

    /// A refusal is only quiet here: the settings section reads the status and says so.
    static func request() async {
        await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
            UNUserNotificationCenter.current().requestAuthorization(options: [.alert]) { _, _ in
                done.resume()
            }
        }
    }
}

/// The per-identity notice setting. A failed read or save keeps the last value and says so.
struct MailNoticeSection: View {
    private static var becameActive: Notification.Name {
        #if os(macOS)
            NSApplication.didBecomeActiveNotification
        #else
            UIApplication.didBecomeActiveNotification
        #endif
    }

    @ObservedObject var feed: ChatFeed
    /// Nil until read: the controls stay disabled.
    @State private var on: Bool?
    /// Kept while off, so a turn-on brings the earlier choice back.
    @State private var content = ChatNotice.fixed
    @State private var failed = false
    @State private var allowed = true
    /// The system shows its question once; after that Allow opens its settings.
    @State private var asked = false

    var body: some View {
        Section {
            Toggle("notice.toggle", isOn: Binding(get: { on ?? false }, set: { save($0, content) }))
                .disabled(on == nil)
                .accessibilityIdentifier("message-notifications")
                .task(id: feed.me) { await load() }
            Picker("notice.content", selection: Binding(get: { content }, set: { save(true, $0) }))
            {
                Text("notice.contentFixed").tag(ChatNotice.fixed)
                Text("notice.contentName").tag(ChatNotice.name)
                Text("notice.contentText").tag(ChatNotice.text)
            }
            #if os(macOS)
                .pickerStyle(.radioGroup)
            #endif
            .disabled(on != true)
            .accessibilityIdentifier("notice-content")
            #if os(macOS)
                Text("notice.hint").font(.caption)
            #else
                Text("notice.hintIos").font(.caption)
            #endif
            if failed {
                Text("notice.failed").font(.caption).accessibilityIdentifier("notice-error")
            }
            if on == true, !allowed {
                #if os(macOS)
                    Text("notice.blocked").font(.caption).accessibilityIdentifier("notice-blocked")
                #else
                    Text("notice.blockedIos").font(.caption).accessibilityIdentifier(
                        "notice-blocked")
                #endif
                Button("notice.allow") { Task { await allow() } }.accessibilityIdentifier(
                    "notice-allow")
            }
        } header: {
            Text("notice.title")
        }
        // The user comes back from the system settings with the answer.
        .onReceive(NotificationCenter.default.publisher(for: Self.becameActive)) { _ in
            Task { await refreshAllowed() }
        }
    }

    private func refreshAllowed() async {
        allowed = await MailNotice.status() == .authorized
    }

    private func load() async {
        await refreshAllowed()
        guard feed.me != nil else { return }
        let read = await feed.call { (try $0.notice(), try $0.noticeContent()) }
        switch read {
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
        } else {
            openSystemSettings()
        }
        await refreshAllowed()
    }

    private func openSystemSettings() {
        #if os(macOS)
            if let settings = URL(
                string: "x-apple.systempreferences:com.apple.preference.notifications")
            {
                NSWorkspace.shared.open(settings)
            }
        #else
            if let settings = URL(string: UIApplication.openSettingsURLString) {
                UIApplication.shared.open(settings, options: [:], completionHandler: nil)
            }
        #endif
    }
}
