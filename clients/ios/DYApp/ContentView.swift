import Foundation
import SwiftUI

private enum ActiveNetworkState: Sendable {
    case loading, needsSetup, ready, readFailed
}

private enum PickerLoad: Sendable {
    case loaded([IdentityInventoryRow]), failed
}

private enum PickerSelection: Sendable {
    case ready(IdentityResolution), failed
}

/// The shared start screen opens a selected identity or asks for an explicit first creation.
struct ContentView: View {
    @State private var identity: IdentityResolution =
        (try? RustBridge.identityState()) ?? .recovery(nil, .unavailable)
    @State private var activeNetwork: ActiveNetworkState = .loading
    @State private var pickerRows: [IdentityInventoryRow] = []
    @State private var pickerLoading = false
    @State private var pickerReadFailed = false
    @State private var pickerWorking = false
    @State private var pickerActionFailed = false

    private var selectedMarker: String? {
        if case let .ready(candidate, _) = identity { return candidate.location.marker }
        return nil
    }

    var body: some View {
        NavigationView {
            // Setup takes the whole column: a lazy form inside the stack hid its lower rows.
            if case let .ready(candidate, peerId) = identity, activeNetwork == .needsSetup {
                NetworkDraftView(candidate: candidate, peerId: peerId, header: AnyView(Group {
                    Text(peerId).accessibilityIdentifier("peer-id")
                    NavigationLink(destination: settingsView(candidate, peerId)) {
                        Text("settings.title")
                    }
                    .accessibilityIdentifier("settings-open")
                }))
            } else {
                start
            }
        }
        .task(id: selectedMarker) { await loadActiveNetwork() }
        .task(id: identity) { await loadPicker() }
    }

    private var start: some View {
        VStack(spacing: 16) {
            Text("app.name").font(.largeTitle)
            switch identity {
            case .setup:
                Text("identity.lossWarning")
                Button("identity.create") { createIdentity() }
                    .accessibilityIdentifier("identity-create")

            case let .ready(candidate, peerId):
                Text(peerId).accessibilityIdentifier("peer-id")
                NavigationLink(destination: settingsView(candidate, peerId)) {
                    Text("settings.title")
                }
                .accessibilityIdentifier("settings-open")
                switch activeNetwork {
                case .loading:
                    ProgressView("network.loading")

                case .needsSetup:
                    EmptyView()

                case .ready:
                    if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                        Text("status.ready").accessibilityIdentifier("app-ready")
                    }
                    NavigationLink(destination: NetworkDraftView(candidate: candidate, peerId: peerId)) {
                        Text("network.title")
                    }
                    .accessibilityIdentifier("network-edit")
                    NavigationLink(destination: ProfileDraftView(candidate: candidate, peerId: peerId)) {
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
                if pickerLoading { ProgressView("settings.loading") }
                if pickerReadFailed {
                    Text("settings.readFailed").accessibilityIdentifier("identity-picker-error")
                }
                ForEach(pickerRows) { row in
                    if let peerID = row.peerID {
                        Text(peerID)
                        Button("settings.select") { Task { await selectPicker(row.candidate.location) } }
                            .disabled(pickerWorking)
                            .accessibilityIdentifier("identity-picker-select-\(row.id)")
                    } else {
                        Text(pickerStatus(for: row.candidate.state))
                    }
                }
                Button("identity.create") { createIdentity() }
                    .disabled(pickerWorking || pickerLoading || pickerReadFailed)
                    .accessibilityIdentifier("identity-picker-create")
                if pickerActionFailed {
                    Text("settings.actionFailed").accessibilityIdentifier("identity-picker-action-error")
                }
                Button("identity.retry") { Task { await loadPicker() } }
                    .accessibilityIdentifier("identity-retry")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func pickerStatus(for state: IdentitySlot.State) -> LocalizedStringKey {
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
    private func loadPicker() async {
        guard case .picker = identity else { return }
        pickerRows = []
        pickerLoading = true
        pickerReadFailed = false
        let result = await Task.detached(priority: .userInitiated) { () -> PickerLoad in
            do { return .loaded(try RustBridge.identityInventory()) }
            catch { return .failed }
        }.value
        guard !Task.isCancelled else { return }
        switch result {
        case .loaded(let rows):
            pickerRows = rows

        case .failed:
            pickerReadFailed = true
        }
        pickerLoading = false
    }

    @MainActor
    private func selectPicker(_ location: IdentityLocation) async {
        guard !pickerWorking else { return }
        pickerWorking = true
        pickerActionFailed = false
        let result = await Task.detached(priority: .userInitiated) { () -> PickerSelection in
            do { return .ready(try RustBridge.selectIdentity(location)) }
            catch { return .failed }
        }.value
        pickerWorking = false
        switch result {
        case .ready(let selected):
            identity = selected

        case .failed:
            pickerActionFailed = true
            await loadPicker()
        }
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

    private func settingsView(_ candidate: IdentityCandidate, _ peerId: String) -> IdentitySettingsView {
        IdentitySettingsView(candidate: candidate, peerId: peerId) { selected in
            activeNetwork = .loading
            identity = selected
        }
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
        if ProcessInfo.processInfo.arguments.contains("--ui-test-corrupt-identity-key"),
           case let .ready(candidate, _) = identity,
           let root = try? RustBridge.directory() {
            let file = RustBridge.networkDirectory(root: root, candidate: candidate)
                .appendingPathComponent("identity.key")
            try? Data("invalid wrapped key".utf8).write(to: file, options: .atomic)
        }
        #endif
    }
}
