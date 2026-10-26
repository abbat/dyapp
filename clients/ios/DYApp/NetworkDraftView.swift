import SwiftUI

private enum NetworkDraftLoad: Sendable {
    case loaded(rows: String, name: String)
    case failed
}

/// Shared first connection form; saving a draft never activates a connection.
struct NetworkDraftView: View {
    let candidate: IdentityCandidate
    @State private var bootstrapRows = ""
    @State private var networkName = ""
    @State private var isLoading = true
    @State private var readFailed = false
    @State private var isSaving = false
    @State private var saveFailed = false
    @State private var hasSaved = false
    @State private var savedRows = ""
    @State private var savedName = ""

    private var hasUnsavedEdits: Bool {
        bootstrapRows != savedRows || networkName != savedName
    }

    private var statusKey: LocalizedStringKey {
        if isLoading { return "network.loading" }
        if readFailed { return "network.readFailed" }
        if isSaving { return "network.saving" }
        if saveFailed { return "network.saveFailed" }
        if hasUnsavedEdits { return "network.unsaved" }
        if hasSaved { return "network.saved" }
        return "network.draftPending"
    }

    var body: some View {
        Form {
            Section {
                TextEditor(text: $bootstrapRows)
                    .frame(minHeight: 120)
                    .disabled(isLoading || readFailed || isSaving)
                    .accessibilityLabel(Text("network.bootstrap"))
                    .accessibilityIdentifier("network-bootstrap")
            } header: {
                Text("network.bootstrap")
            } footer: {
                Text("network.bootstrapHelp")
            }

            Section {
                TextField("network.name", text: $networkName)
                    .disabled(isLoading || readFailed || isSaving)
                    .accessibilityIdentifier("network-name")
            } footer: {
                Text("network.nameHelp")
            }

            Section {
                Button("network.save") { Task { await save() } }
                    .disabled(isLoading || readFailed || isSaving)
                    .accessibilityIdentifier("network-save")
                Text(statusKey)
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
        saveFailed = false
        hasSaved = false
        bootstrapRows = ""
        networkName = ""
        savedRows = ""
        savedName = ""
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
        switch result {
        case let .loaded(rows, name):
            bootstrapRows = rows
            networkName = name
            savedRows = rows
            savedName = name

        case .failed:
            readFailed = true
        }
        isLoading = false
    }

    @MainActor
    private func save() async {
        guard !isLoading && !readFailed && !isSaving else { return }
        isSaving = true
        saveFailed = false
        let selected = candidate
        let rowsText = bootstrapRows
        let name = networkName
        let success = await Task.detached(priority: .userInitiated) {
            do {
                let rows = rowsText.isEmpty ? [] : rowsText.components(separatedBy: "\n")
                try RustBridge.storeNetworkDraft(
                    for: selected, draft: NetworkSettings(seeds: rows, name: name)
                )
                return true
            } catch {
                return false
            }
        }.value
        guard !Task.isCancelled else { return }
        isSaving = false
        saveFailed = !success
        if success {
            savedRows = rowsText
            savedName = name
            hasSaved = true
        }
    }
}
