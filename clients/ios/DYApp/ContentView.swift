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

/// All start screen entries share one selection: on a macOS sidebar a link left active by its own flag
/// kept the detail pane, so after setup opened the profile no other link could replace it. On macOS the
/// selection alone drives the detail column: a link selected soon after Continue left that column empty.
private enum StartLink: Hashable, Sendable {
    case settings, network, profile, search, chats
}

/// The macOS form does not scroll on its own: a long one ran off the top of the window, out of reach.
struct ScrollingForm<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        #if os(macOS)
        ScrollView { Form { content() }.padding() }
        #else
        Form { content() }
        #endif
    }
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
    @State private var profilePending = false
    @State private var startLink: StartLink?
    @StateObject private var search = SearchSession()
    @StateObject private var chats = ChatFeed()

    /// The identity whose mailbox is polled; it takes an active network.
    private var chatIdentity: ChatIdentity? {
        guard case let .ready(candidate, peerId) = identity, activeNetwork == .ready else { return nil }
        return ChatIdentity(candidate: candidate, peerId: peerId)
    }

    private var selectedMarker: String? {
        if case let .ready(candidate, _) = identity { return candidate.location.marker }
        return nil
    }

    var body: some View {
        #if os(macOS)
        VStack(spacing: 0) {
            navigation
            StatusBar(candidate: selectedCandidate)
        }
        #else
        navigation
        #endif
    }

    private var selectedCandidate: IdentityCandidate? {
        if case let .ready(candidate, _) = identity { return candidate }
        return nil
    }

    private var navigation: some View {
        NavigationView {
            Group {
                // Setup takes the whole column: a lazy form inside the stack hid its lower rows.
                if case let .ready(candidate, peerId) = identity, activeNetwork == .needsSetup {
                    NetworkDraftView(candidate: candidate, peerId: peerId, header: AnyView(Group {
                        Text("app.name").font(.largeTitle)
                        Text(peerId).accessibilityIdentifier("peer-id")
                        startEntry(.settings, "settings.title", candidate, peerId)
                            .accessibilityIdentifier("settings-open")
                    })) {
                        // Open the profile from the ready start screen: iOS never pushed from a hidden
                        // link on the setup root, and a macOS sidebar left on the setup form had no way
                        // to search or chats until a restart.
                        profilePending = true
                        activeNetwork = .ready
                    }
                } else {
                    start
                }
            }
            #if os(macOS)
            // The default 140pt sidebar wrapped the setup form word by word and put settings under the splitter.
            .frame(minWidth: 360)
            #endif
            #if os(macOS)
            detail
            #endif
        }
        .task(id: selectedMarker) { await loadActiveNetwork() }
        .task(id: identity) { await loadPicker() }
        // On the root, as iOS cancels a task of a screen covered by a pushed one.
        .task(id: chatIdentity?.peerId) {
            if let chatIdentity { await chats.poll(chatIdentity) }
        }
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
                // Settings opened while loading vanished when setup replaced this layout.
                if activeNetwork != .loading {
                    startEntry(.settings, "settings.title", candidate, peerId)
                        .accessibilityIdentifier("settings-open")
                }
                switch activeNetwork {
                case .loading:
                    ProgressView("network.loading")

                case .needsSetup:
                    EmptyView()

                case .ready:
                    if !ProcessInfo.processInfo.arguments.contains("--break-ui") {
                        Text("status.ready").accessibilityIdentifier("app-ready")
                    }
                    startEntry(.network, "network.title", candidate, peerId)
                        .accessibilityIdentifier("network-edit")
                    startEntry(.profile, "profile.title", candidate, peerId)
                        .accessibilityIdentifier("profile-edit")
                        // Activate the link once it is on screen; one created already active may not push.
                        .onAppear {
                            if profilePending {
                                profilePending = false
                                startLink = .profile
                            }
                        }
                    startEntry(.search, "search.title", candidate, peerId)
                        .accessibilityIdentifier("search-open")
                    startEntry(.chats, "chats.title", candidate, peerId)
                        .accessibilityIdentifier("chats-open")

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

    @ViewBuilder
    private func startEntry(
        _ link: StartLink, _ title: LocalizedStringKey, _ candidate: IdentityCandidate, _ peerId: String
    ) -> some View {
        #if os(macOS)
        Button(title) { startLink = link }
        #else
        NavigationLink(destination: destination(link, candidate, peerId), tag: link, selection: $startLink) {
            Text(title)
        }
        #endif
    }

    @ViewBuilder
    private func destination(_ link: StartLink, _ candidate: IdentityCandidate, _ peerId: String) -> some View {
        switch link {
        case .settings:
            settingsView(candidate, peerId)

        case .network:
            NetworkDraftView(candidate: candidate, peerId: peerId)

        case .profile:
            ProfileDraftView(candidate: candidate, peerId: peerId)

        case .search:
            SearchView(session: search, candidate: candidate, peerId: peerId, feed: chats)

        case .chats:
            ChatsView(feed: chats)
        }
    }

    #if os(macOS)
    @ViewBuilder private var detail: some View {
        if case let .ready(candidate, peerId) = identity, let startLink,
           startLink == .settings ? activeNetwork != .loading : activeNetwork == .ready {
            destination(startLink, candidate, peerId)
        } else {
            Color.clear
        }
    }
    #endif

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
        IdentitySettingsView(candidate: candidate, peerId: peerId, feed: activeNetwork == .ready ? chats : nil) { selected in
            #if os(macOS)
            startLink = nil
            #endif
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
