import SwiftUI

/// The shared start screen opens a selected identity or asks for an explicit first creation.
struct ContentView: View {
    @State private var identity: IdentityResolution =
        (try? RustBridge.identityState()) ?? .recovery(nil, .unavailable)

    var body: some View {
        NavigationView {
            VStack(spacing: 16) {
                Text("app.name").font(.largeTitle)
                switch identity {
                case .setup:
                    Text("identity.lossWarning")
                    Button("identity.create") { createIdentity() }
                        .accessibilityIdentifier("identity-create")

                case .ready(_, let peerId):
                    if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                        Text("status.ready").accessibilityIdentifier("app-ready")
                    }
                    Text(peerId).accessibilityIdentifier("peer-id")
                    NavigationLink(destination: ProfileDraftView(peerId: peerId)) {
                        Text("profile.title")
                    }
                    .accessibilityIdentifier("profile-edit")

                case .recovery:
                    Text("identity.recovery").accessibilityIdentifier("identity-recovery")
                    Button("identity.retry") { refreshIdentity() }
                        .accessibilityIdentifier("identity-retry")

                case .picker:
                    Text("identity.picker").accessibilityIdentifier("identity-picker")
                    Button("identity.retry") { refreshIdentity() }
                        .accessibilityIdentifier("identity-retry")
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func refreshIdentity() {
        identity = (try? RustBridge.identityState()) ?? .recovery(nil, .unavailable)
    }

    private func createIdentity() {
        identity = (try? RustBridge.createIdentity()) ?? .recovery(nil, .unavailable)
    }
}
