import Foundation
import SwiftUI

private enum ActiveNetworkState: Sendable {
    case loading, needsSetup, ready, readFailed
}

private enum PickerLoad: Sendable {
    case loaded([IdentityInventoryRow]), failed
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
    @ObservedObject var identities: IdentitySession
    @State private var identity: IdentityResolution?
    @State private var activeNetwork: ActiveNetworkState = .loading
    @State private var pickerRows: [IdentityInventoryRow] = []
    @State private var pickerLoading = false
    @State private var pickerReadFailed = false
    @State private var pickerWorking = false
    @State private var pickerActionFailed = false
    @State private var pickerRead = UUID()
    @State private var profilePending = false
    @State private var startLink: StartLink?
    @StateObject private var search = SearchSession()
    @StateObject private var chats = ChatFeed()
    @State private var receivingFirstProfile = false

    /// The identity whose mailbox is polled; it takes an active network.
    private var chatIdentity: ChatIdentity? {
        guard case let .ready(candidate, peerId)? = identity, activeNetwork == .ready else {
            return nil
        }
        return ChatIdentity(candidate: candidate, peerId: peerId)
    }

    private var selectedMarker: String? {
        if case let .ready(candidate, _)? = identity { return candidate.location.marker }
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
        if case let .ready(candidate, _)? = identity { return candidate }
        return nil
    }

    private var navigation: some View {
        NavigationView {
            Group {
                // Setup takes the whole column: a lazy form inside the stack hid its lower rows.
                if case let .ready(candidate, peerId)? = identity, activeNetwork == .needsSetup {
                    NetworkDraftView(
                        candidate: candidate, peerId: peerId,
                        header: AnyView(
                            Group {
                                Text("app.name").font(.largeTitle)
                                Text(peerId).accessibilityIdentifier("peer-id")
                                startEntry(.settings, "settings.title", candidate, peerId)
                                    .accessibilityIdentifier("settings-open")
                            })
                    ) {
                        // Open the profile from the ready start screen: iOS never pushed from a hidden
                        // link on the setup root, and a macOS sidebar left on the setup form had no way
                        // to search or chats until a restart.
                        profilePending = !receivingFirstProfile
                        activeNetwork = .ready
                    }
                } else if receivingFirstProfile, activeNetwork == .ready {
                    ScrollingForm {
                        Text("link.receive.title").font(.headline)
                        LinkReceiveSection(feed: chats, wizard: true) { detail in
                            receivingFirstProfile = false
                            identities.reloadAfterSlotChange(detail)
                        }
                        Button("search.cancel") { receivingFirstProfile = false }
                            .accessibilityIdentifier("link-receive-exit")
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
        .safeAreaInset(edge: .top) { persistenceNotice }
        .task { identities.start() }
        .onReceive(identities.$resolution) { useIdentity($0) }
        .task(id: selectedMarker) { await loadActiveNetwork() }
        .task(id: identity) { await loadPicker() }
        // On the root, as iOS cancels a task of a screen covered by a pushed one.
        .task(id: chatIdentity.map { $0.candidate.location.marker + ":" + $0.peerId }) {
            if let chatIdentity {
                await chats.poll(chatIdentity, onRevoked: identities.reloadAfterSlotChange)
            } else {
                chats.deactivate()
            }
        }
    }

    @ViewBuilder private var persistenceNotice: some View {
        if let persistenceDetail = identities.persistenceDetail {
            VStack(alignment: .leading, spacing: 8) {
                Text("identity.persistence.notice").font(.caption)
                Text(verbatim: persistenceDetail).font(.caption).textSelection(.enabled)
                Button("identity.persistence.ack", action: identities.acknowledgePersistence)
            }
            .padding()
            .accessibilityIdentifier("identity-persistence-notice")
        }
    }

    private var start: some View {
        VStack(spacing: 16) {
            Text("app.name").font(.largeTitle)
            switch identity {
            case nil:
                ProgressView("settings.loading").accessibilityIdentifier("identity-loading")

            case .some(.setup):
                Text("identity.lossWarning")
                Button("identity.create") { Task { await createIdentity() } }
                    .disabled(identities.busy)
                    .accessibilityIdentifier("identity-create")
                Button("link.receive.title") {
                    receivingFirstProfile = true
                    Task {
                        await createIdentity()
                        if pickerActionFailed { receivingFirstProfile = false }
                    }
                }
                .disabled(identities.busy)
                .accessibilityIdentifier("identity-receive")
                if pickerActionFailed {
                    Text("settings.actionFailed").accessibilityIdentifier("identity-create-error")
                }

            case let .some(.ready(candidate, peerId)):
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

            case .some(.recovery):
                Text("identity.recovery").accessibilityIdentifier("identity-recovery")
                Button("identity.retry") { refreshIdentity() }
                    .accessibilityIdentifier("identity-retry")

            case .some(.picker):
                Text("identity.picker").accessibilityIdentifier("identity-picker")
                if pickerLoading { ProgressView("settings.loading") }
                if pickerReadFailed {
                    Text("settings.readFailed").accessibilityIdentifier("identity-picker-error")
                }
                ForEach(pickerRows) { row in
                    if let peerID = row.peerID {
                        Text(peerID)
                        Button("settings.select") {
                            Task { await selectPicker(row.candidate.location) }
                        }
                        .disabled(identities.busy || pickerWorking)
                        .accessibilityIdentifier("identity-picker-select-\(row.id)")
                    } else {
                        Text(pickerStatus(for: row.candidate.state))
                    }
                }
                Button("identity.create") { Task { await createIdentity() } }
                    .disabled(identities.busy || pickerWorking || pickerLoading || pickerReadFailed)
                    .accessibilityIdentifier("identity-picker-create")
                if pickerActionFailed {
                    Text("settings.actionFailed").accessibilityIdentifier(
                        "identity-picker-action-error")
                }
                Button("identity.retry") { Task { await loadPicker() } }
                    .accessibilityIdentifier("identity-retry")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    #if os(macOS)
        @ViewBuilder private var detail: some View {
            if case let .ready(candidate, peerId)? = identity, let startLink,
                startLink == .settings ? activeNetwork != .loading : activeNetwork == .ready
            {
                destination(startLink, candidate, peerId)
            } else {
                Color.clear
            }
        }
    #endif

    /// The macOS detail column selects the sidebar's network entry, as a push there has no way back on
    /// macOS 12. During setup the sidebar is that form, so only the detail closes. iOS pushes instead.
    private var openNetwork: (() -> Void)? {
        #if os(macOS)
            return { startLink = activeNetwork == .needsSetup ? nil : .network }
        #else
            return nil
        #endif
    }

}

extension ContentView {
    @ViewBuilder
    private func startEntry(
        _ link: StartLink, _ title: LocalizedStringKey, _ candidate: IdentityCandidate,
        _ peerId: String
    ) -> some View {
        #if os(macOS)
            Button(title) { startLink = link }
        #else
            NavigationLink(
                destination: destination(link, candidate, peerId), tag: link, selection: $startLink
            ) {
                Text(title)
            }
        #endif
    }

    @ViewBuilder
    private func destination(_ link: StartLink, _ candidate: IdentityCandidate, _ peerId: String)
        -> some View
    {
        switch link {
        case .settings:
            settingsView(candidate, peerId)

        case .network:
            #if os(macOS)
                // A push from the macOS 12 detail column has no way back: select the profile instead.
                NetworkDraftView(candidate: candidate, peerId: peerId) { startLink = .profile }
            #else
                NetworkDraftView(candidate: candidate, peerId: peerId)
            #endif

        case .profile:
            ProfileDraftView(candidate: candidate, peerId: peerId)
                .id(candidate.location.marker + ":" + peerId)

        case .search:
            SearchView(
                session: search, candidate: candidate, peerId: peerId, feed: chats,
                openNetwork: openNetwork)

        case .chats:
            ChatsView(feed: chats)
        }
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
        guard case .picker? = identity else { return }
        let ticket = UUID()
        pickerRead = ticket
        pickerRows = []
        pickerLoading = true
        pickerReadFailed = false
        let result = await IdentityIO.performBlocking { () -> PickerLoad in
            do { return .loaded(try RustBridge.identityInventory()) } catch { return .failed }
        }
        guard !Task.isCancelled, pickerRead == ticket else { return }
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
        guard case .picker? = identity, !pickerWorking, !identities.busy else { return }
        pickerWorking = true
        pickerActionFailed = false
        let succeeded = await identities.perform { try RustBridge.selectIdentity(location) }
        guard !Task.isCancelled else { return }
        pickerWorking = false
        if !succeeded {
            pickerActionFailed = true
            await loadPicker()
        }
    }

    @MainActor
    private func loadActiveNetwork() async {
        guard case let .ready(candidate, peerId)? = identity else { return }
        activeNetwork = .loading
        let result = await IdentityIO.performBlocking { () -> ActiveNetworkState in
            do {
                return try RustBridge.loadNetworkSettings(
                    for: candidate, peerId: peerId, file: .active) == nil
                    ? .needsSetup : .ready
            } catch {
                return .readFailed
            }
        }
        guard !Task.isCancelled else { return }
        activeNetwork = result
    }

    private func refreshIdentity() { identities.reload() }

    private func useIdentity(_ selected: IdentityResolution?) {
        if identity != selected {
            chats.deactivate()
            search.retire()
            activeNetwork = .loading
            startLink = nil
            profilePending = false
            pickerRead = UUID()
        }
        identity = selected
    }

    private func settingsView(_ candidate: IdentityCandidate, _ peerId: String)
        -> IdentitySettingsView
    {
        var view = IdentitySettingsView(
            candidate: candidate, peerId: peerId, feed: activeNetwork == .ready ? chats : nil,
            identities: identities)
        view.openNetwork = openNetwork
        return view
    }

    @MainActor
    private func createIdentity() async {
        guard identity == identities.resolution else { return }
        switch identity {
        case .some(.setup), .some(.picker):
            break

        default:
            return
        }
        pickerActionFailed = false
        let succeeded = await identities.perform { try RustBridge.createIdentity() }
        guard !Task.isCancelled else { return }
        pickerActionFailed = !succeeded
    }
}
