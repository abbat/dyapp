import SwiftUI

/// Expanding this private list does not fetch public profiles, names or media from the network.
struct SavedProfilesBlock: View {
    @ObservedObject var feed: ChatFeed
    @StateObject private var model: SavedProfilesModel
    @State private var expanded = false
    let open: (String) -> Void

    var body: some View {
        Button(LocalizedStringKey(expanded ? "favorites.hide" : "favorites.show")) {
            expanded.toggle()
        }
        .accessibilityIdentifier("saved-profiles-toggle")
        if expanded {
            content
                .task {
                    model.activate()
                    await model.refresh(owner: model.owner)
                }
                .onDisappear { model.retire() }
        }
    }

    private var content: some View {
        let expected = model.owner
        return VStack(alignment: .leading, spacing: 12) {
            Text("favorites.listHint").font(.caption)
            if model.busy { Text("favorites.listLoading").font(.caption) }
            if let status = model.status {
                Text(LocalizedStringKey(status)).accessibilityIdentifier("saved-profiles-status")
            }
            if model.peers?.isEmpty == true {
                Text("favorites.empty").font(.caption).accessibilityIdentifier(
                    "saved-profiles-empty")
            }
            ForEach(model.peers ?? [], id: \.self) { peer in
                let label = contactLabel(peer, feed.names[peer])
                HStack {
                    Button {
                        guard model.canOpen(peer, owner: expected) else { return }
                        open(peer)
                    } label: {
                        Text(verbatim: label).frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .accessibilityIdentifier("saved-open-\(peer)")
                    Button("favorites.remove") {
                        Task { await model.refresh(owner: expected, removing: peer) }
                    }
                    .accessibilityLabel(
                        Text(
                            verbatim: String(
                                format: NSLocalizedString("favorites.removeNamed", comment: ""),
                                label))
                    )
                    .accessibilityIdentifier("saved-remove-\(peer)")
                }
            }
            Button("favorites.refresh") {
                Task { await model.refresh(owner: expected) }
            }
            .accessibilityIdentifier("saved-profiles-refresh")
        }
        .buttonStyle(.borderless)
        .disabled(model.busy)
    }

    init(feed: ChatFeed, open: @escaping (String) -> Void) {
        self.feed = feed
        self.open = open
        _model = StateObject(wrappedValue: SavedProfilesModel(feed: feed))
    }
}
