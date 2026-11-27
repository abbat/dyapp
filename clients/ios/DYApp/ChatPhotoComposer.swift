import SwiftUI
import UniformTypeIdentifiers

@MainActor
struct ChatPhotoComposer: View {
    @ObservedObject var feed: ChatFeed
    let peer: String
    let context: ChatFeed.Context
    let disabled: Bool
    @State private var picking = false
    @State private var working = false
    @State private var draft: PreparedPhoto?
    @State private var confirming = false
    @State private var status: ChatStatus?

    var body: some View {
        VStack(alignment: .leading) {
            Button("profile.photoAdd") { picking = true }
                .disabled(disabled || working)
                .accessibilityIdentifier("chat-photo-attach")
            if working { ProgressView() }
            if let status {
                Text(LocalizedStringKey(status.key))
                    .accessibilityIdentifier("chat-photo-send-status")
            }
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.jpeg, .png]) { result in
            guard case let .success(url) = result else { return }
            working = true
            status = nil
            Task { await prepare(url) }
        }
        .sheet(isPresented: $confirming, onDismiss: { draft = nil }) {
            ScrollingForm {
                if let draft, let image = (try? checkPreview(bytes: draft.thumbnail)).flatMap(previewImage) {
                    image.resizable().scaledToFit()
                        .accessibilityLabel(Text("chat.photo"))
                }
                Button("chat.send") {
                    send()
                    confirming = false
                }
                .accessibilityIdentifier("chat-photo-confirm")
                Button("search.cancel", role: .cancel) { confirming = false }
            }
        }
        .onChange(of: feed.owner) { _ in
            if !feed.isCurrent(context) { confirming = false }
        }
    }

    private func prepare(_ url: URL) async {
        let result = await IdentityIO.performBlocking {
            Result { try PhotoPrep.prepare(PhotoPrep.read(url)) }
        }
        guard feed.isCurrent(context) else { return }
        working = false
        switch result {
        case let .success(photo):
            draft = photo
            confirming = true

        case .failure:
            status = ChatStatus(key: "chat.sendFailed", details: nil)
        }
    }

    private func send() {
        guard !working, feed.isCurrent(context), let photo = draft else { return }
        working = true
        draft = nil
        let target = peer
        Task {
            let result = await feed.call(context) {
                try $0.submitChatPhoto(
                    peerId: target, photo: photo.full, preview: photo.thumbnail,
                    width: photo.width, height: photo.height)
            }
            guard feed.isCurrent(context) else { return }
            status = chatSendStatus(result)
            working = false
            await feed.read()
        }
    }
}
