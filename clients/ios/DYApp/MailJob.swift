@preconcurrency import BackgroundTasks
@preconcurrency import UserNotifications

/// Ends a background task once: its time can run out while the receive still runs.
private final class TaskEnd: @unchecked Sendable {
    private let task: BGTask
    private let lock = NSLock()
    private var ended = false

    init(_ task: BGTask) { self.task = task }

    func callAsFunction(success: Bool) {
        lock.lock()
        defer { lock.unlock() }
        guard !ended else { return }
        ended = true
        task.setTaskCompleted(success: success)
    }
}

/// Receives the active identity's mail in the windows iOS gives a closed app, as Android's job does.
/// The key opens after the first unlock, so this also runs while the app lock is on; that lock only
/// covers the screens.
enum MailJob {
    /// Also listed in Info.plist: iOS stops the app when it registers one that is not.
    static let identifier = "com.dyapp.ios.mail"
    /// As on Android; iOS decides the real time from how the app is used.
    private static let period: TimeInterval = 15 * 60

    /// Call during launch: iOS allows registering only until launch ends.
    static func register() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: identifier, using: nil) { run($0) }
    }

    /// A simulator refuses and a user may turn background refresh off; the app then receives while open.
    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: period)
        try? BGTaskScheduler.shared.submit(request)
    }

    private static func run(_ task: BGTask) {
        schedule()
        let end = TaskEnd(task)
        // shortcut: a receive in Rust cannot be interrupted, so one longer than the window of about 30 s
        // waits suspended and its notice may be lost; measure a receive on a device if notices go missing.
        task.expirationHandler = { end(success: false) }
        Task.detached(priority: .utility) {
            await receiveMail()
            end(success: true)
        }
    }

    /// One receive for the active ready identity, then a notice when it left more to read; none, no
    /// network or a failure stays quiet.
    private static func receiveMail() async {
        guard case let .ready(candidate, peerId) = try? RustBridge.identityState(),
            (try? RustBridge.loadNetworkSettings(for: candidate, file: .active)) != nil
        else { return }
        let identity = ChatIdentity(candidate: candidate, peerId: peerId)
        let pass = try? RustBridge.withChats(identity) { session in
            let (rows, liked) = (try session.chatRows(), try session.likedYou())
            _ = try session.receive()
            return (
                rows, liked, try session.chatRows(), try session.likedYou(), try session.notice()
            )
        }
        guard let (rows, liked, newRows, newLiked, notice) = pass, notice != .off,
            MailNotice.count(rows: newRows, liked: newLiked)
                > MailNotice.count(rows: rows, liked: liked)
        else { return }
        let lines = MailNotice.lines(notice, before: rows, after: newRows) { peer in
            let view = (try? RustBridge.profileSearcher(for: candidate)).flatMap {
                try? RustBridge.contactNames(identity, searcher: $0, peers: [peer]).first
            }
            return contactLabel(peer, view)
        }
        let center = UNUserNotificationCenter.current()
        let settings = await withCheckedContinuation {
            (done: CheckedContinuation<(UNAuthorizationStatus, UNShowPreviewsSetting), Never>) in
            center.getNotificationSettings {
                done.resume(returning: ($0.authorizationStatus, $0.showPreviewsSetting))
            }
        }
        guard settings.0 == .authorized else { return }
        try? await center.add(MailNotice.request(shown(lines, previews: settings.1)))
    }

    /// iOS alone decides what its lock screen shows, and an app cannot hide a notice there, so what the
    /// setting adds goes out only while iOS keeps previews off the lock screen.
    static func shown(_ lines: MailNotice.Lines?, previews: UNShowPreviewsSetting) -> MailNotice
        .Lines?
    {
        previews == .always ? nil : lines
    }
}
