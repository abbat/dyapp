import Foundation
import SwiftUI

private enum ActiveNetworkState: Sendable {
    case loading, needsSetup, ready, readFailed
}

/// The shared start screen opens a selected identity or asks for an explicit first creation.
struct ContentView: View {
    @State private var identity: IdentityResolution =
        (try? RustBridge.identityState()) ?? .recovery(nil, .unavailable)
    @State private var activeNetwork: ActiveNetworkState = .loading

    private var selectedMarker: String? {
        if case let .ready(candidate, _) = identity { return candidate.location.marker }
        return nil
    }

    var body: some View {
        NavigationView {
            VStack(spacing: 16) {
                Text("app.name").font(.largeTitle)
                switch identity {
                case .setup:
                    Text("identity.lossWarning")
                    Button("identity.create") { createIdentity() }
                        .accessibilityIdentifier("identity-create")

                case let .ready(candidate, peerId):
                    Text(peerId).accessibilityIdentifier("peer-id")
                    switch activeNetwork {
                    case .loading:
                        ProgressView("network.loading")

                    case .needsSetup:
                        NetworkDraftView(candidate: candidate, peerId: peerId)

                    case .ready:
                        if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                            Text("status.ready").accessibilityIdentifier("app-ready")
                        }
                        NavigationLink(destination: NetworkDraftView(candidate: candidate, peerId: peerId)) {
                            Text("network.title")
                        }
                        .accessibilityIdentifier("network-edit")
                        NavigationLink(destination: ProfileDraftView(peerId: peerId)) {
                            Text("profile.title")
                        }
                        .accessibilityIdentifier("profile-edit")

                    case .readFailed:
                        Text("network.activeReadFailed").accessibilityIdentifier("network-active-error")
                        Button("identity.retry") { Task { await loadActiveNetwork() } }
                            .accessibilityIdentifier("network-active-retry")
                    }

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
        .task(id: selectedMarker) { await loadActiveNetwork() }
    }

    @MainActor
    private func loadActiveNetwork() async {
        guard case let .ready(candidate, _) = identity else { return }
        activeNetwork = .loading
        let result = await Task.detached(priority: .userInitiated) { () -> ActiveNetworkState in
            do {
                return try RustBridge.loadNetworkSettings(for: candidate, file: .active) == nil
                    ? .needsSetup : .ready
            } catch {
                return .readFailed
            }
        }.value
        guard !Task.isCancelled else { return }
        activeNetwork = result
    }

    private func refreshIdentity() {
        identity = (try? RustBridge.identityState()) ?? .recovery(nil, .unavailable)
    }

    private func createIdentity() {
        identity = (try? RustBridge.createIdentity()) ?? .recovery(nil, .unavailable)
        #if DEBUG
        // Native UI tests exercise the read-error screen in their isolated slot root.
        if ProcessInfo.processInfo.arguments.contains("--ui-test-corrupt-network-draft"),
           case let .ready(candidate, _) = identity,
           let root = try? RustBridge.directory() {
            let draft = RustBridge.networkDirectory(root: root, candidate: candidate)
                .appendingPathComponent("network-draft")
            try? Data("invalid network file\n".utf8).write(to: draft, options: .atomic)
        }
        #endif
    }
}
