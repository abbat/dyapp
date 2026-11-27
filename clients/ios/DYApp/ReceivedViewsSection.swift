import SwiftUI

struct ReceivedViewsSection: View {
    @ObservedObject var feed: ChatFeed
    let open: (String) -> Void
    @State private var rows: [ReceivedProfileView]?
    @State private var busy = false
    @State private var failed = false
    @State private var expanded = false
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
            Button("privacy.views.refresh") { Task { await refresh(opening: expected) } }
                .disabled(busy)
        }
        .accessibilityIdentifier("received-views")
        .task(id: expected) {
            if expanded { await refresh(opening: expected) }
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

    private func refresh(opening expected: UUID) async {
        guard opening == expected, expanded, !busy, !Task.isCancelled, let context = feed.context
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
