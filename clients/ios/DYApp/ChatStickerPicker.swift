import SwiftUI

@MainActor
struct ChatStickerPicker: View {
    @ObservedObject var feed: ChatFeed
    let context: ChatFeed.Context
    let peer: String
    let disabled: Bool
    @State private var showing = false

    var body: some View {
        Button("sticker.picker.title") { showing = true }
            .disabled(disabled)
            .accessibilityIdentifier("chat-sticker-picker")
            .sheet(isPresented: $showing) {
                RecentStickersSheet(feed: feed, context: context, peer: peer)
            }
    }
}

@MainActor
private struct RecentStickersSheet: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var feed: ChatFeed
    let context: ChatFeed.Context
    let peer: String
    @State private var entries: [RecentStickerPickerEntry] = []
    @State private var packs: [SavedStickerPackEntry] = []
    @State private var packCursor: Data?
    @State private var packsLoaded = false
    @State private var selectedPack: Data?
    @State private var cursor: UInt32? = 0
    @State private var busy = false
    @State private var committed = false
    @State private var status: ChatStatus?

    var body: some View {
        ScrollingForm {
            Text("sticker.picker.title").font(.headline)
            if busy { ProgressView() }
            if let status { Text(LocalizedStringKey(status.key)) }
            if !committed {
                if selectedPack != nil {
                    Button("sticker.picker.title") { selectPack(nil) }.disabled(busy)
                } else {
                    ForEach(packs, id: \.packId) { pack in
                        if case let .ready(name, _) = pack.state {
                            Button {
                                selectPack(pack.packId)
                            } label: {
                                Text(verbatim: name)
                            }
                            .disabled(busy)
                        }
                    }
                    if !packsLoaded || packCursor != nil {
                        Button("search.more") { Task { await loadPacks() } }.disabled(busy)
                    }
                }
                ForEach(entries.indices, id: \.self) { index in
                    let entry = entries[index]
                    Button {
                        send(entry)
                    } label: {
                        HStack {
                            let sticker = entry.sticker
                            if let image = stickerImage(
                                width: sticker.width, height: sticker.height, pixels: sticker.pixels
                            ) {
                                image.resizable().scaledToFit().frame(width: 48, height: 48)
                                    .accessibilityHidden(true)
                            }
                            Text(verbatim: "\(sticker.emoji) \(entry.packName)")
                        }
                    }
                    .disabled(busy)
                }
                if let cursor, !busy {
                    Button(LocalizedStringKey(entries.isEmpty ? "search.retry" : "search.more")) {
                        Task { await load(cursor) }
                    }
                } else if entries.isEmpty, !busy, selectedPack == nil {
                    Text("stickerPicker.noRecent")
                }
            }
            Button("search.cancel") { dismiss() }.disabled(busy)
        }
        .interactiveDismissDisabled(busy)
        .task {
            await load(0)
            await loadPacks()
        }
        .onChange(of: feed.owner) { _ in
            if !feed.isCurrent(context) { dismiss() }
        }
    }

    private func selectPack(_ pack: Data?) {
        selectedPack = pack
        cursor = 0
        entries = []
        status = nil
        Task { await load(0) }
    }

    private func loadPacks() async {
        guard !busy, !packsLoaded || packCursor != nil else { return }
        busy = true
        let after = packCursor
        let result = await feed.call(context) { try $0.savedStickerPacks(after: after) }
        guard !Task.isCancelled, feed.isCurrent(context) else { return }
        busy = false
        switch result {
        case let .success(page):
            packs.append(contentsOf: page.entries)
            packCursor = page.next
            packsLoaded = true

        case .failure:
            status = ChatStatus(key: "chat.loadFailed", details: nil)
        }
    }

    private func load(_ after: UInt32) async {
        guard !busy, cursor == after else { return }
        busy = true
        let pack = selectedPack
        let result = await feed.call(context) { chats in
            guard let pack else { return try chats.recentStickerPage(after: after) }
            let page = try chats.savedStickerPage(packId: pack, after: after)
            return RecentStickerPickerPage(
                entries: page.entries.map {
                    RecentStickerPickerEntry(packId: pack, packName: page.name, sticker: $0)
                },
                next: page.next)
        }
        guard !Task.isCancelled, feed.isCurrent(context) else { return }
        busy = false
        switch result {
        case let .success(page):
            entries.append(contentsOf: page.entries)
            cursor = page.next
            status = nil

        case .failure:
            status = ChatStatus(key: "chat.loadFailed", details: nil)
        }
    }

    private func send(_ entry: RecentStickerPickerEntry) {
        guard !busy, !committed, feed.isCurrent(context) else { return }
        busy = true
        let pack = entry.packId
        let index = entry.sticker.index
        let target = peer
        Task {
            let result = await feed.call(context) {
                try $0.submitSavedSticker(peerId: target, packId: pack, index: index)
            }
            guard feed.isCurrent(context) else { return }
            busy = false
            status = chatSendStatus(result)
            if case .success = result {
                committed = true
                if status == nil { dismiss() }
                await feed.read()
            }
        }
    }
}
