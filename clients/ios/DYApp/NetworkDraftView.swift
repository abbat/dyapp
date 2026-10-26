import SwiftUI

private enum NetworkDraftLoad: Sendable {
    case loaded(rows: String, name: String)
    case failed
}

/// Shared first connection form. Loading and saving the selected identity's draft follow separately.
struct NetworkDraftView: View {
    let candidate: IdentityCandidate
    @State private var bootstrapRows = ""
    @State private var networkName = ""
    @State private var isLoading = true
    @State private var readFailed = false

    var body: some View {
        Form {
            Section {
                TextEditor(text: $bootstrapRows)
                    .frame(minHeight: 120)
                    .disabled(isLoading || readFailed)
                    .accessibilityLabel(Text("network.bootstrap"))
                    .accessibilityIdentifier("network-bootstrap")
            } header: {
                Text("network.bootstrap")
            } footer: {
                Text("network.bootstrapHelp")
            }

            Section {
                TextField("network.name", text: $networkName)
                    .disabled(isLoading || readFailed)
                    .accessibilityIdentifier("network-name")
            } footer: {
                Text("network.nameHelp")
            }

            Section {
                Text(LocalizedStringKey(isLoading ? "network.loading" :
                    (readFailed ? "network.readFailed" : "network.draftPending")))
                    .accessibilityIdentifier("network-draft-status")
            }
        }
        .navigationTitle("network.title")
        .task(id: candidate.location.marker) { await load() }
    }

    @MainActor
    private func load() async {
        isLoading = true
        readFailed = false
        bootstrapRows = ""
        networkName = ""
        let selected = candidate
        let result = await Task.detached(priority: .userInitiated) { () -> NetworkDraftLoad in
            do {
                let settings = try RustBridge.loadNetworkSettings(for: selected, file: .draft)
                    ?? RustBridge.loadNetworkSettings(for: selected, file: .active)
                return .loaded(rows: settings?.seeds.joined(separator: "\n") ?? "",
                               name: settings?.name ?? "")
            } catch {
                return .failed
            }
        }.value
        guard !Task.isCancelled else { return }
        isLoading = false
        switch result {
        case let .loaded(rows, name):
            bootstrapRows = rows
            networkName = name

        case .failed:
            readFailed = true
        }
    }
}
