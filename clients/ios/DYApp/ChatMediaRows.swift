import SwiftUI

@MainActor
struct ChatStickerRow: View {
    @ObservedObject var feed: ChatFeed
    @ObservedObject private var network = NetworkType.shared
    let context: ChatFeed.Context
    let peer: String
    let sticker: ChatSticker
    @State private var frame: Image?
    @State private var manual = false
    @State private var failed = false
    @State private var confirming = false
    @State private var deleting = false
    @State private var deleteStatus: ChatStatus?

    var body: some View {
        VStack(alignment: .leading) {
            if let frame {
                frame.resizable().scaledToFit().frame(maxWidth: 120, maxHeight: 120)
                    .accessibilityHidden(true)
            }
            Text(verbatim: "\(sticker.emoji) \(sticker.packName)")
                .accessibilityIdentifier("msg-sticker")
            if frame == nil {
                if failed {
                    Text("stickerImage.failed").font(.caption)
                        .accessibilityIdentifier("msg-sticker-error")
                }
                Button("stickerImage.load") {
                    failed = false
                    manual = true
                }
                    .disabled(manual)
                    .accessibilityIdentifier("msg-sticker-load")
            }
            if sticker.author == feed.me {
                Button("chat.delete", role: .destructive) { confirming = true }
                    .disabled(deleting)
                    .accessibilityIdentifier("msg-sticker-delete")
            }
            if sticker.author != feed.me {
                ReceivedStickerPack(feed: feed, context: context, source: ChatForwardSource(
                    peerId: peer, groupId: sticker.groupId, author: sticker.author,
                    messageId: sticker.id))
            }
            if let deleteStatus { Text(LocalizedStringKey(deleteStatus.key)) }
        }
        .confirmationDialog("delete.title", isPresented: $confirming, titleVisibility: .visible) {
            Button("chat.delete", role: .destructive, action: deleteSticker)
        } message: {
            Text("delete.text")
        }
        .task(id: network.metered) { await load(manually: false) }
        .task(id: manual) { if manual { await load(manually: true) } }
    }

    private func deleteSticker() {
        guard !deleting, feed.isCurrent(context) else { return }
        deleting = true
        let (peer, group, author, id) = (peer, sticker.groupId, sticker.author, sticker.id)
        Task {
            let result = await feed.call(context) {
                try $0.submitDeleteSticker(
                    peerId: peer, groupId: group, author: author, messageId: id)
            }
            guard feed.isCurrent(context) else { return }
            deleteStatus = chatSendStatus(result)
            if case .success = result {
                await feed.read()
            } else {
                deleting = false
            }
        }
    }

    private func load(manually: Bool) async {
        guard frame == nil else { return }
        let (peer, group, author, id) = (peer, sticker.groupId, sticker.author, sticker.id)
        let result = await feed.call(context) {
            try $0.chatStickerImage(
                peerId: peer, groupId: group, author: author, id: id, manual: manually)
        }
        guard !Task.isCancelled, feed.isCurrent(context) else { return }
        if manually { manual = false }
        if case let .success(.ready(pixels)) = result, let image = stickerImage(
            width: pixels.width, height: pixels.height, pixels: pixels.pixels)
        {
            frame = image
            failed = false
        } else if manually {
            failed = true
        }
    }
}

@MainActor
private struct ReceivedStickerPack: View {
    @ObservedObject var feed: ChatFeed
    let context: ChatFeed.Context
    let source: ChatForwardSource
    @State private var status: StickerPackSaveStatus?
    @State private var busy = false
    @State private var failed = false

    var body: some View {
        Group { controls }
            .task(id: feed.version) {
                guard !busy else { return }
                await read()
            }
    }

    @ViewBuilder private var controls: some View {
        if let status, status.controls.contains(.request) {
            Button("sticker.pack.request") { update(.request) }.disabled(busy)
        }
        if let status, let generation = status.generation {
            if status.controls.contains(.retry) {
                Button("search.retry") { update(.retry(generation: generation)) }.disabled(busy)
            }
            if status.controls.contains(.fresh) {
                Button("stickerRequest.fresh") { update(.fresh(generation: generation)) }
                    .disabled(busy)
            }
            if status.controls.contains(.stopWaiting) {
                Button("stickerRequest.stop") { update(.stopWaiting(generation: generation)) }
                    .disabled(busy)
            }
        }
        if failed || status?.controls.contains(.refresh) == true {
            Button("search.retry") { update(nil) }.disabled(busy)
        }
    }

    private func read() async {
        let selector = source
        let result = await feed.call(context) {
            try $0.chatStickerPackStatus(
                peerId: selector.peerId, groupId: selector.groupId, author: selector.author,
                id: selector.messageId)
        }
        guard !Task.isCancelled, feed.isCurrent(context) else { return }
        switch result {
        case let .success(current):
            status = current
            failed = false

        case .failure:
            status = nil
            failed = true
        }
    }

    private func update(_ action: StickerPackSaveAction?) {
        guard !busy, feed.isCurrent(context) else { return }
        busy = true
        let selector = source
        Task {
            if let action {
                let result = await feed.call(context) {
                    try $0.changeStickerPackRequest(
                        peerId: selector.peerId, groupId: selector.groupId, author: selector.author,
                        id: selector.messageId, action: action)
                }
                guard feed.isCurrent(context) else { return }
                if case let .success(.ready(current)) = result {
                    status = current
                    failed = false
                } else {
                    status = nil
                    failed = true
                }
            } else {
                await read()
            }
            busy = false
        }
    }
}

@MainActor
struct ChatPhotoPreview: View {
    let bytes: Data
    @ObservedObject var feed: ChatFeed
    @ObservedObject private var network = NetworkType.shared
    let opening: PhotoOpening
    @State private var image: Image?
    @State private var original: Image?

    var body: some View {
        Group {
            if let image = original ?? image {
                image.resizable().scaledToFit().frame(maxWidth: 120, maxHeight: 120)
                    .accessibilityHidden(true)
            }
        }
        .task(id: bytes) {
            image = nil
            let input = bytes
            if !input.isEmpty {
                let decoded = await IdentityIO.performBlocking {
                    DecodedPhoto(image: (try? checkPreview(bytes: input)).flatMap(previewImage))
                }
                guard !Task.isCancelled, feed.isCurrent(opening.context) else { return }
                image = decoded.image
            }
        }
        .task(id: network.metered) {
            guard original == nil else { return }
            let loaded = await loadChatPhoto(feed: feed, opening: opening, automatic: true)
            guard !Task.isCancelled else { return }
            original = loaded
        }
    }
}

struct PhotoOpening: Identifiable {
    let id = UUID()
    let source: ChatForwardSource
    let context: ChatFeed.Context
}

struct ChatPhotoViewer: View {
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var feed: ChatFeed
    let opening: PhotoOpening
    @State private var image: Image?
    @State private var failed = false
    @State private var attempt = 0

    var body: some View {
        ScrollingForm {
            if let image {
                image.resizable().scaledToFit()
                    .accessibilityLabel(Text("chat.photo"))
                    .accessibilityIdentifier("msg-photo-original")
            } else if failed {
                Text("view.photoFailed").accessibilityIdentifier("msg-photo-error")
                Button("search.retry") {
                    failed = false
                    attempt += 1
                }
            } else {
                ProgressView("view.photoLoading")
            }
            Button("search.cancel") { dismiss() }
        }
        .task(id: attempt) { await load() }
        .onChange(of: feed.owner) { _ in
            if !feed.isCurrent(opening.context) { dismiss() }
        }
    }

    private func load() async {
        let loaded = await loadChatPhoto(feed: feed, opening: opening)
        guard !Task.isCancelled, feed.isCurrent(opening.context) else { return }
        image = loaded
        failed = loaded == nil
    }
}

@MainActor
private func loadChatPhoto(
    feed: ChatFeed, opening: PhotoOpening, automatic: Bool = false
) async -> Image? {
    let source = opening.source
    let result = await feed.call(opening.context) {
        if automatic {
            return try $0.chatPhotoAutomatic(
                peerId: source.peerId, groupId: source.groupId,
                author: source.author, id: source.messageId)
        }
        return try $0.chatPhotoOriginal(
            peerId: source.peerId, groupId: source.groupId,
            author: source.author, id: source.messageId)
    }
    guard !Task.isCancelled, feed.isCurrent(opening.context),
        case let .success(bytes) = result
    else { return nil }
    let decoded = await IdentityIO.performBlocking {
        DecodedPhoto(image: fullPhotoImage(bytes, allowPNG: true))
    }
    guard !Task.isCancelled, feed.isCurrent(opening.context) else { return nil }
    return decoded.image
}
