import SwiftUI

private enum InventoryLoad: Sendable {
    case loaded([IdentityInventoryRow], published: Bool), failed
}

/// Inventory and explicit identity actions for the selected Apple key slot.
struct IdentitySettingsView: View {
    @Environment(\.dismiss) private var dismiss
    let candidate: IdentityCandidate
    let peerId: String
    /// The chat store of the active identity, once its network is set up.
    let feed: ChatFeed?
    @ObservedObject var identities: IdentitySession
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
    @State private var opening = UUID()

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
        let currentOpening = opening
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
                                    Task {
                                        guard isCurrent(currentOpening) else { return }
                                        await select(row.candidate.location)
                                    }
                                }
                                .disabled(isWorking || identities.busy)
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
                Button("settings.create") {
                    Task {
                        guard isCurrent(currentOpening) else { return }
                        await create()
                    }
                }
                .disabled(isLoading || readFailed || isWorking || identities.busy)
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
                    .disabled(isLoading || readFailed || isWorking || identities.busy)
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
                Button("identity.retry") {
                    Task {
                        guard isCurrent(currentOpening) else { return }
                        await withdrawThenRemove(id)
                    }
                }
                Button("settings.removeAnyway", role: .destructive) {
                    Task {
                        guard isCurrent(currentOpening) else { return }
                        await remove(id)
                    }
                }
            } else {
                Button("settings.removeConfirm", role: .destructive) {
                    Task {
                        guard isCurrent(currentOpening) else { return }
                        await withdrawThenRemove(id)
                    }
                }
            }
        } message: { _ in
            removalMessage
        }
        .task { await load() }
        .onDisappear {
            opening = UUID()
            isWorking = false
        }
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
        let currentOpening = opening
        let result = await IdentityIO.performBlocking { () -> InventoryLoad in
            do {
                // Only an open key can sign the withdrawal; an unreadable version counts as never published.
                let version =
                    (try? RustBridge.visibility(for: current, peerId: currentPeerID))?.version ?? 0
                return .loaded(try RustBridge.identityInventory(), published: version > 0)
            } catch { return .failed }
        }
        guard !Task.isCancelled, isCurrent(currentOpening) else { return }
        switch result {
        case let .loaded(items, published):
            rows = items
            hasPublished = published

        case .failed:
            readFailed = true
        }
        isLoading = false
    }

    private func isCurrent(_ expectedOpening: UUID) -> Bool {
        opening == expectedOpening && identities.resolution == .ready(candidate, peerId)
    }

    @MainActor
    private func create() async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let currentOpening = opening
        let succeeded = await identities.perform { try RustBridge.createIdentity() }
        guard !Task.isCancelled, opening == currentOpening else { return }
        finish(succeeded)
    }

    @MainActor
    private func select(_ location: IdentityLocation) async {
        guard !isWorking else { return }
        isWorking = true
        actionFailed = false
        let currentOpening = opening
        let succeeded = await identities.perform { try RustBridge.selectIdentity(location) }
        guard !Task.isCancelled, opening == currentOpening else { return }
        finish(succeeded)
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
        let currentOpening = opening
        let withdrawn = await IdentityIO.performBlocking {
            (try? RustBridge.withdrawProfile(for: current, peerId: currentPeerID)) != nil
        }
        guard !Task.isCancelled, isCurrent(currentOpening) else { return }
        isWorking = false
        if withdrawn {
            await remove(id)
        } else {
            // A failure can return at once, and an alert asked for while the last one closes is lost.
            try? await Task.sleep(nanoseconds: 400_000_000)
            guard !Task.isCancelled, isCurrent(currentOpening) else { return }
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
        let currentOpening = opening
        let succeeded = await identities.perform {
            try RustBridge.removeIdentity(id, confirming: confirmedPeerID)
        }
        guard !Task.isCancelled, opening == currentOpening else { return }
        isWorking = false
        if succeeded || identities.resolution != .ready(candidate, peerId) {
            close()
        } else {
            actionFailed = true
            await load()
        }
    }

    @MainActor
    private func finish(_ succeeded: Bool) {
        isWorking = false
        if succeeded {
            close()
        } else {
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
