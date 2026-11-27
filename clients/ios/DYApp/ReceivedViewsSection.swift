import SwiftUI

struct ReceivedViewsScreen: View {
    @ObservedObject var feed: ChatFeed
    @State private var profile: String?
    @State private var chat: String?

    var body: some View {
        Group {
            if let profile {
                FoundProfileView(
                    peer: profile, known: nil, feed: feed,
                    back: { self.profile = nil },
                    message: { showChat(profile) },
                    openChat: showChat, blocked: clearRoute
                )
                .id(profile)
            } else if let chat {
                ChatView(
                    feed: feed, peer: chat,
                    profile: { self.profile = chat },
                    openProfile: { self.profile = $0 },
                    back: { self.chat = nil },
                    blocked: { self.chat = nil }
                )
                .id(chat)
            } else {
                ScrollingForm {
                    ReceivedViewsSection(feed: feed) { self.profile = $0 }
                }
            }
        }
        .navigationTitle("privacy.views.received.title")
    }

    private func clearRoute() {
        (profile, chat) = (nil, nil)
    }

    private func showChat(_ peer: String) {
        (profile, chat) = (nil, peer)
    }
}

struct ReceivedViewsSection: View {
    @ObservedObject var feed: ChatFeed
    let open: (String) -> Void
    @State private var rows: [ReceivedProfileView]?
    @State private var busy = false
    @State private var failed = false
    @State private var expanded = true
    @State private var opening = UUID()

    var body: some View {
        let expected = opening
        let context = feed.context
        DisclosureGroup(
            "privacy.views.received.title",
            isExpanded: Binding(get: { expanded }, set: setExpanded)
        ) {
            Text("privacy.views.received.hint").font(.caption)
            if let rows {
                ForEach(rows, id: \.peerId) { row in
                    Button {
                        guard let context, feed.isCurrent(context), opening == expected,
                            expanded, !busy,
                            self.rows?.contains(where: { $0.peerId == row.peerId }) == true
                        else { return }
                        open(row.peerId)
                    } label: {
                        VStack(alignment: .leading) {
                            Text(verbatim: row.peerId)
                            Text(verbatim: row.utcDate).font(.caption)
                        }
                    }
                    .disabled(busy)
                    .accessibilityIdentifier("received-view-" + row.peerId)
                }
                if rows.isEmpty {
                    Text("privacy.views.empty").accessibilityIdentifier("received-views-empty")
                }
            }
            if busy {
                ProgressView("privacy.views.loading").accessibilityIdentifier(
                    "received-views-loading")
            }
            if failed {
                Text("privacy.views.failed").accessibilityIdentifier("received-views-error")
            }
            Button("privacy.views.refresh") {
                guard let context else { return }
                Task { await refresh(context, opening: expected) }
            }
            .disabled(busy || context == nil)
        }
        .accessibilityIdentifier("received-views")
        .task(id: expected) {
            if expanded, let context { await refresh(context, opening: expected) }
        }
        .onDisappear {
            opening = UUID()
            busy = false
        }
    }

    private func setExpanded(_ value: Bool) {
        guard expanded != value else { return }
        opening = UUID()
        busy = false
        expanded = value
    }

    private func refresh(_ context: ChatFeed.Context, opening expected: UUID) async {
        guard feed.isCurrent(context), opening == expected, expanded, !busy, !Task.isCancelled
        else { return }
        busy = true
        let result = await feed.call(context) { try $0.receivedProfileViews() }
        guard opening == expected, feed.isCurrent(context) else { return }
        busy = false
        guard !Task.isCancelled, expanded else { return }
        switch result {
        case let .success(read):
            rows = read
            failed = false

        case .failure:
            failed = true
        }
    }
}
