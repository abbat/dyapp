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
        }
        .task(id: network.metered) { await load(manually: false) }
        .task(id: manual) { if manual { await load(manually: true) } }
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
