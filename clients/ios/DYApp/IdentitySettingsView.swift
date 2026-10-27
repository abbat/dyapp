import SwiftUI

private enum InventoryLoad: Sendable {
    case loaded([IdentityInventoryRow]), failed
}

private enum IdentityAction: Sendable {
    case ready(IdentityCandidate, String), failed
}

private enum RemovalAction: Sendable {
    case resolved(IdentityResolution), failed(IdentityResolution)
}

/// Inventory and explicit identity actions for the selected Apple key slot.
struct IdentitySettingsView: View {
    @Environment(\.dismiss) private var dismiss
    let candidate: IdentityCandidate
    let peerId: String
    let onIdentityChanged: (IdentityResolution) -> Void
    @State private var rows: [IdentityInventoryRow] = []
    @State private var isLoading = true
    @State private var readFailed = false
    @State private var isWorking = false
    @State private var actionFailed = false
    @State private var showRemoveConfirmation = false

    private var removalWarning: LocalizedStringKey {
        rows.count == 1 ? "settings.removeLastWarning" : "settings.removeWarning"
    }

    private var removableSlot: UUID? {
        guard case .slot(let id) = candidate.location,
              rows.first(where: { $0.candidate.location == candidate.location })?.peerID == peerId else {
            return nil
        }
        return id
    }

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
                            if row.peerID != nil && row.candidate.location != candidate.location {
                                Button("settings.select") { Task { await select(row.candidate.location) } }
                                    .disabled(isWorking)
                                    .accessibilityIdentifier("settings-select-\(row.id)")
                            }
                        }
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

            Section {
                Text("settings.createWarning")
                Button("settings.create") { Task { await create() } }
                    .disabled(isLoading || readFailed || isWorking)
                    .accessibilityIdentifier("settings-create")
                if isWorking { ProgressView("settings.working") }
                if actionFailed {
                    Text("settings.actionFailed").accessibilityIdentifier("settings-action-error")
                }
            }

            if let id = removableSlot {
                Section {
                    Button("settings.remove", role: .destructive) {
                        showRemoveConfirmation = true
                    }
                    .disabled(isLoading || readFailed || isWorking)
                    .accessibilityIdentifier("settings-remove")
                }
                .alert("settings.removeTitle", isPresented: $showRemoveConfirmation) {
                    Button("settings.removeCancel", role: .cancel) {}
                    Button("settings.removeConfirm", role: .destructive) {
                        Task { await remove(id) }
                    }
                } message: {
                    Text(peerId) + Text(verbatim: "\n") + Text(removalWarning)
                }
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

    @MainActor
    private func create() async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let result = await Task.detached(priority: .userInitiated) { () -> IdentityAction in
            do {
                if case let .ready(created, peerID) = try RustBridge.createIdentity() {
                    return .ready(created, peerID)
                }
            } catch { return .failed }
            return .failed
        }.value
        finish(result)
    }

    @MainActor
    private func select(_ location: IdentityLocation) async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let result = await Task.detached(priority: .userInitiated) { () -> IdentityAction in
            do {
                if case let .ready(selected, peerID) = try RustBridge.selectIdentity(location) {
                    return .ready(selected, peerID)
                }
            } catch { return .failed }
            return .failed
        }.value
        finish(result)
    }

    @MainActor
    private func remove(_ id: UUID) async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let confirmedPeerID = peerId
        let result = await Task.detached(priority: .userInitiated) { () -> RemovalAction in
            do { return .resolved(try RustBridge.removeIdentity(id, confirming: confirmedPeerID)) }
            catch {
                return .failed((try? RustBridge.identityState()) ?? .recovery(nil, .unavailable))
            }
        }.value
        isWorking = false
        switch result {
        case .resolved(let resolution):
            onIdentityChanged(resolution)
            dismiss()

        case .failed(let resolution):
            if resolution != .ready(candidate, peerId) {
                onIdentityChanged(resolution)
                dismiss()
            } else {
                actionFailed = true
                await load()
            }
        }
    }

    @MainActor
    private func finish(_ result: IdentityAction) {
        isWorking = false
        switch result {
        case let .ready(selected, peerID):
            onIdentityChanged(.ready(selected, peerID))
            dismiss()

        case .failed:
            actionFailed = true
        }
    }
}
