import SwiftUI

private enum InventoryLoad: Sendable {
    case loaded([IdentityInventoryRow]), failed
}

/// Read-only inventory of local identities for the selected Apple key slot.
struct IdentitySettingsView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var rows: [IdentityInventoryRow] = []
    @State private var isLoading = true
    @State private var readFailed = false

    var body: some View {
        Form {
            Section {
                if isLoading {
                    ProgressView("settings.loading")
                } else if readFailed {
                    Text("settings.readFailed").accessibilityIdentifier("settings-error")
                    Button("identity.retry") { Task { await load() } }
                } else {
                    ForEach(rows) { row in
                        VStack(alignment: .leading) {
                            if let peerID = row.peerID {
                                Text(peerID)
                            } else {
                                Text(statusKey(for: row.candidate.state))
                            }
                            Text("settings.keychain")
                            if row.candidate.location == candidate.location {
                                Text("settings.active")
                            }
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier(row.candidate.location == candidate.location
                                                 ? "settings-active" : "settings-identity-\(row.id)")
                    }
                }
            } header: {
                Text("settings.identities")
            }

            Section {
                NavigationLink(destination: NetworkDraftView(candidate: candidate, peerId: peerId)) {
                    Text("network.title")
                }
                .accessibilityIdentifier("settings-network")
            }
        }
        .navigationTitle("settings.title")
        .task { await load() }
    }

    private func statusKey(for state: IdentitySlot.State) -> LocalizedStringKey {
        switch state {
        case .pending:
            "settings.pending"

        case .ready:
            "settings.unavailable"

        case .deleting:
            "settings.deleting"

        case .damaged:
            "settings.damaged"
        }
    }

    @MainActor
    private func load() async {
        isLoading = true
        readFailed = false
        let result = await Task.detached(priority: .userInitiated) { () -> InventoryLoad in
            do { return .loaded(try RustBridge.identityInventory()) }
            catch { return .failed }
        }.value
        guard !Task.isCancelled else { return }
        switch result {
        case .loaded(let items):
            rows = items

        case .failed:
            readFailed = true
        }
        isLoading = false
    }
}
