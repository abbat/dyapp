import SwiftUI

private enum InventoryLoad: Sendable {
    case loaded([IdentityInventoryRow], published: Bool), failed
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
    /// The chat store of the active identity, once its network is set up.
    let feed: ChatFeed?
    let onIdentityChanged: (IdentityResolution) -> Void
    /// Replaces the network link where the start screen owns that navigation.
    var openNetwork: (() -> Void)?
    @State private var rows: [IdentityInventoryRow] = []
    @State private var isLoading = true
    @State private var readFailed = false
    @State private var isWorking = false
    @State private var actionFailed = false
    @State private var showRemoveConfirmation = false
    /// This identity has published, so removal withdraws its profile first.
    @State private var hasPublished = false
    @State private var withdrawFailed = false

    private var removalWarning: LocalizedStringKey {
        rows.count == 1 ? "settings.removeLastWarning" : "settings.removeWarning"
    }

    private var removalMessage: Text {
        let warning = Text(peerId) + Text(verbatim: "\n") + Text(removalWarning)
        guard hasPublished else { return warning }
        let next: LocalizedStringKey =
            withdrawFailed ? "settings.removeOffline" : "settings.removeWithdraws"
        return warning + Text(verbatim: "\n") + Text(next)
    }

    private var removableSlot: UUID? {
        guard case .slot(let id) = candidate.location,
            rows.first(where: { $0.candidate.location == candidate.location })?.peerID == peerId
        else {
            return nil
        }
        return id
    }

    var body: some View {
        ScrollingForm {
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
                            .accessibilityIdentifier(
                                row.candidate.location == candidate.location
                                    ? "settings-active" : "settings-identity-\(row.id)")
                            if row.peerID != nil && row.candidate.location != candidate.location {
                                Button("settings.select") {
                                    Task { await select(row.candidate.location) }
                                }
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
                if let openNetwork {
                    Button("network.title", action: openNetwork)
                        .accessibilityIdentifier("settings-network")
                } else {
                    NavigationLink(
                        destination: NetworkDraftView(candidate: candidate, peerId: peerId)
                    ) {
                        Text("network.title")
                    }
                    .accessibilityIdentifier("settings-network")
                }
            }

            #if os(macOS)
                MenuBarSection()
                StorageSection(candidate: candidate)
            #endif

            if let feed {
                MailNoticeSection(feed: feed)
                ReadReceiptsSection(feed: feed)
                PeopleSections(feed: feed)
            }

            UpdatesSection()

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

            if removableSlot != nil {
                Section {
                    Button("settings.remove", role: .destructive) {
                        withdrawFailed = false
                        showRemoveConfirmation = true
                    }
                    .disabled(isLoading || readFailed || isWorking)
                    .accessibilityIdentifier("settings-remove")
                }
            }
        }
        .navigationTitle("settings.title")
        // Keep the presenter outside the conditional, scrolling Form section on macOS.
        .alert(
            "settings.removeTitle", isPresented: $showRemoveConfirmation, presenting: removableSlot
        ) { id in
            Button("settings.removeCancel", role: .cancel) {}
            if withdrawFailed {
                Button("identity.retry") { Task { await withdrawThenRemove(id) } }
                Button("settings.removeAnyway", role: .destructive) { Task { await remove(id) } }
            } else {
                Button("settings.removeConfirm", role: .destructive) {
                    Task { await withdrawThenRemove(id) }
                }
            }
        } message: { _ in
            removalMessage
        }
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
        let (current, currentPeerID) = (candidate, peerId)
        let result = await Task.detached(priority: .userInitiated) { () -> InventoryLoad in
            do {
                // Only an open key can sign the withdrawal; an unreadable version counts as never published.
                let version =
                    (try? RustBridge.visibility(for: current, peerId: currentPeerID))?.version ?? 0
                return .loaded(try RustBridge.identityInventory(), published: version > 0)
            } catch { return .failed }
        }.value
        guard !Task.isCancelled else { return }
        switch result {
        case let .loaded(items, published):
            rows = items
            hasPublished = published

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

    /// The tombstone goes first; when no node takes it, the person chooses to wait or remove anyway.
    @MainActor
    private func withdrawThenRemove(_ id: UUID) async {
        guard hasPublished else {
            await remove(id)
            return
        }
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let (current, currentPeerID) = (candidate, peerId)
        let withdrawn = await Task.detached(priority: .userInitiated) { () -> Bool in
            (try? RustBridge.withdrawProfile(for: current, peerId: currentPeerID)) != nil
        }.value
        isWorking = false
        if withdrawn {
            await remove(id)
        } else {
            // A failure can return at once, and an alert asked for while the last one closes is lost.
            try? await Task.sleep(nanoseconds: 400_000_000)
            withdrawFailed = true
            showRemoveConfirmation = true
        }
    }

    @MainActor
    private func remove(_ id: UUID) async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let confirmedPeerID = peerId
        let result = await Task.detached(priority: .userInitiated) { () -> RemovalAction in
            do {
                return .resolved(try RustBridge.removeIdentity(id, confirming: confirmedPeerID))
            } catch {
                return .failed((try? RustBridge.identityState()) ?? .recovery(nil, .unavailable))
            }
        }.value
        isWorking = false
        switch result {
        case .resolved(let resolution):
            onIdentityChanged(resolution)
            close()

        case .failed(let resolution):
            if resolution != .ready(candidate, peerId) {
                onIdentityChanged(resolution)
                close()
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
            close()

        case .failed:
            actionFailed = true
        }
    }

    /// On macOS these settings fill the detail column, where dismiss closes the whole window;
    /// the identity change clears the selection that shows this detail instead.
    private func close() {
        #if os(iOS)
            dismiss()
        #endif
    }
}
