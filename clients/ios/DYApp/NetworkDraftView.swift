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
    @State private var isChecking = false
    @State private var checkFailed = false
    @State private var checkedRowsText = ""
    @State private var checkedName = ""
    @State private var rowOutcomes: [NetworkRowOutcome] = []

    private var hasUnsavedEdits: Bool {
        bootstrapRows != savedRows || networkName != savedName
    }

    private var showingCheck: Bool {
        bootstrapRows == checkedRowsText && networkName == checkedName && !rowOutcomes.isEmpty
    }

    private var checkStatusKey: LocalizedStringKey {
        if isChecking { return "network.checking" }
        if checkFailed { return "network.checkFailed" }
        if showingCheck {
            let connected = rowOutcomes.contains { outcome in
                if case .connected = outcome { return true }
                return false
            }
            return connected ? "network.connected" : "network.unreachable"
        }
        return "network.checkReady"
    }

    private var rowStatusKey: (NetworkRowOutcome) -> LocalizedStringKey {
        { outcome in
            switch outcome {
            case .connected:
                return "network.row.connected"

            case .invalidAddress:
                return "network.row.invalidAddress"

            case .invalidPeerID:
                return "network.row.invalidPeerID"

            case .duplicate:
                return "network.row.duplicate"

            case .unreachable:
                return "network.row.unreachable"

            case .wrongPeer:
                return "network.row.wrongPeer"

            case .wrongNetwork:
                return "network.row.wrongNetwork"
            }
        }
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
                    .disabled(isLoading || readFailed || isSaving || isChecking)
                    .accessibilityLabel(Text("network.bootstrap"))
                    .accessibilityIdentifier("network-bootstrap")
            } header: {
                Text("network.bootstrap")
            } footer: {
                Text("network.bootstrapHelp")
            }

            Section {
                TextField("network.name", text: $networkName)
                    .disabled(isLoading || readFailed || isSaving || isChecking)
                    .accessibilityIdentifier("network-name")
            } footer: {
                Text("network.nameHelp")
            }

            Section {
                Button("network.save") { Task { await save() } }
                    .disabled(isLoading || readFailed || isSaving || isChecking)
                    .accessibilityIdentifier("network-save")
                Text(statusKey)
                    .accessibilityIdentifier("network-draft-status")
            }

            Section {
                Button("network.check") { Task { await check() } }
                    .disabled(isLoading || readFailed || isSaving || isChecking)
                    .accessibilityIdentifier("network-check")
                Text(checkStatusKey).accessibilityIdentifier("network-check-status")
                if showingCheck {
                    ForEach(rowOutcomes.indices, id: \.self) { index in
                        VStack(alignment: .leading) {
                            Text(checkedRowsText.components(separatedBy: "\n")[index])
                            Text(rowStatusKey(rowOutcomes[index]))
                            if case let .connected(peerID) = rowOutcomes[index] {
                                Text(peerID)
                            }
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("network-row-\(index)")
                    }
                }
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
        checkFailed = false
        rowOutcomes = []
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
        guard !isLoading && !readFailed && !isSaving && !isChecking else { return }
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

    @MainActor
    private func check() async {
        guard !isLoading && !readFailed && !isSaving && !isChecking else { return }
        isChecking = true
        checkFailed = false
        rowOutcomes = []
        let rowsText = bootstrapRows
        let name = networkName
        let rows = rowsText.isEmpty ? [] : rowsText.components(separatedBy: "\n")
        let result = await NetworkConnectionSession.start(candidate: candidate, rows: rows, name: name)
        guard !Task.isCancelled else { return }
        checkedRowsText = rowsText
        checkedName = name
        isChecking = false
        switch result {
        case let .completed(_, outcomes):
            rowOutcomes = outcomes
            savedRows = rowsText
            savedName = name
            hasSaved = true
            saveFailed = false

        case .failed:
            checkFailed = true
        }
    }
}
