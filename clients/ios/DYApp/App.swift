import SwiftUI

/// Main app entry point for DYApp iOS
@main
struct DYAppApp: App {
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            LockGate()
        }
        .onChange(of: phase) { if $0 == .background { MailJob.schedule() } }
    }

    init() {
        NetworkType.startReporting()
        MailJob.register()
    }
}
