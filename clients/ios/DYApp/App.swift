import SwiftUI

/// Main app entry point for DYApp iOS
@main
struct DYAppApp: App {
    @Environment(\.scenePhase) private var phase
    @StateObject private var identities = IdentitySession()

    var body: some Scene {
        WindowGroup {
            LockGate(identities: identities)
        }
        .onChange(of: phase) { if $0 == .background { MailJob.schedule() } }
    }

    init() {
        NetworkType.startReporting()
        MailJob.register()
    }
}
