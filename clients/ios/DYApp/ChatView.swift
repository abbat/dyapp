import SwiftUI

/// A line under the thread; a failure that stored nothing has details and keeps the draft.
struct ChatStatus: Equatable {
    let key: String
    let details: String?
}

/// One conversation, shown by state inside Chats: a link in the macOS 12 detail column has no way back.
struct ChatView: View {
    @ObservedObject var feed: ChatFeed
    let peer: String
    let profile: () -> Void
    /// A shared profile's card was pressed.
    let openProfile: (String) -> Void
    let back: () -> Void
    /// The block held, so the chat is gone.
    let blocked: () -> Void
    @State private var history: ChatThread?
    private var items: [ChatItem] { history?.items ?? [] }
    @StateObject private var composer = ChatComposer()
    @State private var deleting: ChatMessage?
    @State private var forwarding: ForwardOpening?
    @State private var viewingPhoto: PhotoOpening?
    @State private var confirmClear = false
    @State private var confirmBlock = false
    @State private var renaming = false
    @State private var comparing = false
    /// A clear, mute or block is running.
    @State private var busy = false

    private var messages: [ChatMessage] {
        items.compactMap { item in
            guard case .text(let message) = item else { return nil }
            return message
        }
    }
    private var sending: Bool { composer.sending }
    private var editing: Data? { composer.editing }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button("chats.title", action: back).accessibilityIdentifier("chat-back")
            header
            notice
            // Each dialog sits on its own view, so one never shadows another.
            thread.confirmationDialog(
                "block.title", isPresented: $confirmBlock, titleVisibility: .visible
            ) {
                Button("chat.block", role: .destructive, action: block)
            } message: {
                Text("block.text")
            }
            statusLines
            composerInput
            if history?.state == .open, let context = feed.context {
                ChatPhotoComposer(feed: feed, peer: peer, context: context, disabled: busy || sending)
            }
        }
        .sheet(item: $forwarding) { opening in
            ChatForwardPicker(feed: feed, opening: opening)
        }
        .sheet(item: $viewingPhoto) { opening in
            ChatPhotoViewer(feed: feed, opening: opening)
        }
        .padding()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
        // The feed counts its reads, so a received message and a finished send both land here.
        .task(id: feed.version) { await reload() }
        .task(id: peer) { await feed.loadNames([peer]) }
        .onDisappear {
            composer.retire()
            busy = false
        }
        .sheet(isPresented: $renaming) {
            // The field starts from the local name: a public name is the peer's own claim.
            ChatRenameSheet(
                name: feed.names[peer].flatMap { $0.verified == nil ? $0.name : nil } ?? ""
            ) {
                await feed.rename(peer, name: $0)
            }
        }
        .confirmationDialog(
            "delete.title",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible,
            presenting: deleting
        ) { message in
            Button("chat.delete", role: .destructive) { delete(message) }
        } message: { _ in
            Text("delete.text")
        }
    }

    private var header: some View {
        HStack {
            Button(action: profile) {
                HStack {
                    PeerAvatar(photos: feed.photos, peer: peer, hash: feed.names[peer]?.photo)
                    Text(verbatim: contactLabel(peer, feed.names[peer])).font(.headline)
                }
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("chat-profile")
            Spacer()
            moreMenu
        }
        .confirmationDialog("clear.title", isPresented: $confirmClear, titleVisibility: .visible) {
            Button("clear.action", role: .destructive, action: clear)
        } message: {
            Text("clear.text") + Text(verbatim: " ") + Text("clear.later")
        }
        .sheet(isPresented: $comparing) { SafetyNumberSheet(me: feed.me, peer: peer) }
    }

    private var moreMenu: some View {
        Menu {
            Button("rename.title") { renaming = true }.accessibilityIdentifier("chat-rename")
            Button("safety.action") { comparing = true }.accessibilityIdentifier("chat-safety")
            Button("chat.clear") { confirmClear = true }.accessibilityIdentifier("chat-clear")
            // A request offers its block on the notice instead.
            if history?.state != .requestReceived {
                Button("chat.block", role: .destructive) { confirmBlock = true }
                    .accessibilityIdentifier("chat-block")
            }
            // A peer with no chat yet has nothing to mute.
            if let history, history.state != ChatState.none {
                Button(LocalizedStringKey(history.muted ? "chat.unmute" : "chat.mute")) {
                    setMuted(!history.muted)
                }
                .accessibilityIdentifier("chat-mute")
            }
        } label: {
            Image(systemName: "ellipsis.circle")
        }
        .fixedSize()
        .disabled(busy || sending)
        .accessibilityLabel(Text("chat.more"))
        .accessibilityIdentifier("chat-more")
    }

    @ViewBuilder private var notice: some View {
        switch history?.state {
        case ChatState.none?, .requestSent?:
            Text("chat.firstNotice").font(.caption).accessibilityIdentifier("chat-first-notice")

        case .requestReceived?:
            Text("chat.requestNotice").font(.caption).accessibilityIdentifier("chat-request-notice")
            Button("chat.block", role: .destructive) { confirmBlock = true }
                .disabled(busy)
                .accessibilityIdentifier("request-block")

        case .closed?:
            Text("chat.closedNotice").font(.caption).accessibilityIdentifier("chat-closed-notice")

        case .open?, nil:
            EmptyView()
        }
    }

    private var thread: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 8) {
                    ForEach(items, id: \.rowID) { item in
                        switch item {
                        case .text(let message):
                            textBubble(message)

                        case .photo(let photo):
                            photoBubble(photo)

                        case .sticker(let sticker):
                            Text(verbatim: "\(sticker.emoji) \(sticker.packName)")
                        }
                    }
                }
            }
            .onChange(of: items.last?.rowID) { last in
                if let last { proxy.scrollTo(last, anchor: .bottom) }
            }
        }
        .accessibilityIdentifier("chat-thread")
    }

    private func textBubble(_ message: ChatMessage) -> some View {
        ChatBubble(
            message: message, own: message.author == feed.me,
            writable: chatWritable(history?.state), sending: sending || busy,
            openProfile: openProfile,
            edit: { startEditing(message) }, delete: { deleting = message },
            forward: {
                openForward(group: message.groupId, author: message.author, id: message.id, photo: false)
            }
        )
    }

    private func photoBubble(_ photo: ChatPhoto) -> some View {
        VStack(alignment: .leading) {
            if history?.state == .open, let context = feed.context {
                ChatPhotoPreview(
                    bytes: photo.preview, feed: feed,
                    opening: PhotoOpening(source: photoSource(photo), context: context))
            }
            if photo.forwarded {
                Text(verbatim: String(
                    format: NSLocalizedString("chats.forwarded", comment: ""),
                    NSLocalizedString("chat.photo", comment: "")))
                    .font(.caption)
                    .accessibilityIdentifier("msg-photo-forwarded")
            } else {
                Text("chat.photo").accessibilityIdentifier("msg-photo")
            }
            Button("view.photoLoad") {
                guard let context = feed.context else { return }
                viewingPhoto = PhotoOpening(source: photoSource(photo), context: context)
            }
            .accessibilityIdentifier("msg-photo-load")
            Menu {
                Button("chat.forward") {
                    openForward(group: photo.groupId, author: photo.author, id: photo.id, photo: true)
                }
                .accessibilityIdentifier("msg-forward")
                .disabled(sending || busy)
            } label: {
                Image(systemName: "ellipsis").frame(minWidth: 44, minHeight: 44)
            }
            .accessibilityLabel(Text("msg.menu"))
            .accessibilityIdentifier("msg-photo-menu")
        }
    }

    private func photoSource(_ photo: ChatPhoto) -> ChatForwardSource {
        ChatForwardSource(peerId: peer, groupId: photo.groupId, author: photo.author, messageId: photo.id)
    }

    private func openForward(group: Data, author: String, id: Data, photo: Bool) {
        guard let context = feed.context else { return }
        let source = ChatForwardSource(peerId: peer, groupId: group, author: author, messageId: id)
        forwarding = ForwardOpening(source: source, photo: photo, context: context)
    }

    @ViewBuilder private var statusLines: some View {
        if history?.state == .requestSent { Text("chat.firstWait").font(.caption) }
        if sending {
            Text("chat.sending").font(.caption).accessibilityIdentifier("chat-status")
        } else if let status = composer.status {
            Text(LocalizedStringKey(status.key)).font(.caption).accessibilityIdentifier(
                "chat-status")
            if let details = status.details {
                Text(verbatim: details).font(.caption).textSelection(.enabled)
                    .accessibilityIdentifier("chat-details")
            }
        }
        if chatComposerPolicy(state: history?.state, text: composer.draft).tooLong {
            Text("chat.tooLong").font(.caption)
        }
        if editing != nil {
            HStack {
                Text("chat.editing").font(.caption).accessibilityIdentifier("chat-editing")
                Button("search.cancel", action: composer.endEditing).accessibilityIdentifier(
                    "chat-edit-cancel")
            }
        }
    }

    // shortcut: one line of input, as multi-line TextField needs macOS 13 and iOS 16; use it when the targets rise.
    private var composerInput: some View {
        HStack {
            TextField("chat.placeholder", text: $composer.draft)
                .disabled(!chatWritable(history?.state))
                .onSubmit(send)
                .accessibilityIdentifier("chat-input")
            Button("chat.send", action: send)
                .disabled(busy || sending || !chatSendable(history?.state, composer.draft))
                .accessibilityIdentifier("chat-send")
        }
    }

    private func reload() async {
        guard let context = feed.context else { return }
        let opening = composer.opening
        let peer = peer
        let result = await feed.call(context) { try $0.chatThread(peerId: peer) }
        guard !Task.isCancelled, feed.isCurrent(context), composer.opening == opening else {
            return
        }
        switch result {
        case let .success(loaded):
            if history == nil { composer.status = nil }
            history = loaded
            await feed.markRead(peer, in: context)

        case let .failure(error):
            if history == nil {
                composer.status = ChatStatus(
                    key: "chat.loadFailed", details: String(describing: error))
            }
        }
    }

    private func send() {
        guard !busy, let context = feed.context,
            let submission = composer.begin(state: history?.state, messages: messages)
        else { return }
        let (feed, peer) = (feed, peer)
        Task { [weak composer] in
            let result: Result<ChatSubmission, Error>
            if let target = submission.target {
                result = await feed.edit(target, text: submission.text, peer: peer, in: context)
            } else {
                result = await feed.submit(submission.text, to: peer, in: context)
            }
            guard feed.isCurrent(context) else { return }
            composer?.complete(submission, result: result)
        }
    }

    private func startEditing(_ message: ChatMessage) {
        guard !busy, !sending else { return }
        composer.edit(message)
    }

    private func delete(_ message: ChatMessage) {
        guard !busy, !sending, let context = feed.context else { return }
        let (opening, edit) = (composer.opening, composer.edit)
        busy = true
        Task {
            let result = await feed.delete(message, peer: peer, in: context)
            guard feed.isCurrent(context), composer.opening == opening else { return }
            busy = false
            composer.status = chatSendStatus(
                result, queued: "chat.deleteQueued", failed: "chat.deleteFailed")
            // A deleted message has nothing left to edit.
            if case .success = result, editing == message.threadKey {
                composer.endEditing(ifUnchanged: edit)
            }
        }
    }

    private func clear() {
        guard !busy, !sending, let context = feed.context else { return }
        let (opening, edit) = (composer.opening, composer.edit)
        busy = true
        Task {
            let failed = await feed.clear(peer, in: context)
            guard feed.isCurrent(context), composer.opening == opening else { return }
            composer.status = failed ?? ChatStatus(key: "chat.cleared", details: nil)
            if failed == nil { composer.endEditing(ifUnchanged: edit) }
            busy = false
        }
    }

    private func setMuted(_ muted: Bool) {
        guard !busy, !sending, let context = feed.context else { return }
        let opening = composer.opening
        busy = true
        Task {
            let status = await feed.setMuted(muted, peer: peer, in: context)
            guard feed.isCurrent(context), composer.opening == opening else { return }
            composer.status = status
            // The menu stays off until its label can tell the new state.
            await reload()
            guard feed.isCurrent(context), composer.opening == opening else { return }
            busy = false
        }
    }

    private func block() {
        guard !busy, !sending, let context = feed.context else { return }
        let opening = composer.opening
        busy = true
        Task {
            let status = await feed.block(peer, in: context)
            guard feed.isCurrent(context), composer.opening == opening else { return }
            composer.status = status
            busy = false
            if status == nil { blocked() }
        }
    }
}

private struct ChatBubble: View {
    let message: ChatMessage
    let own: Bool
    let writable: Bool
    let sending: Bool
    let openProfile: (String) -> Void
    let edit: () -> Void
    let delete: () -> Void
    let forward: () -> Void

    var body: some View {
        HStack {
            if own { Spacer(minLength: 48) }
            if own, !message.deleted { menu }
            VStack(alignment: .leading, spacing: 4) {
                if let shared = sharedProfile(message) {
                    // It names only the short Peer ID: no message fetches anything before the press.
                    Button {
                        openProfile(shared)
                    } label: {
                        Label(chatPreview(message), systemImage: "person.crop.circle")
                    }
                    .accessibilityIdentifier("msg-profile")
                } else {
                    Text(verbatim: chatPreview(message)).textSelection(.enabled)
                        .accessibilityIdentifier("msg-text")
                }
                let meta = chatMeta(message)
                if !meta.isEmpty { Text(verbatim: meta).font(.caption) }
            }
            .padding(8)
            .background(
                RoundedRectangle(cornerRadius: 12).fill(
                    own ? Color.accentColor.opacity(0.2) : Color.secondary.opacity(0.15)
                ))
            if !own, !message.deleted { menu }
            if !own { Spacer(minLength: 48) }
        }
    }

    // A button rather than a context menu: right click on selectable text opens the text's own menu.
    private var menu: some View {
        Menu {
            Button("chat.forward", action: forward).disabled(sending)
                .accessibilityIdentifier("msg-forward")
            if own, writable {
                Button("chat.edit", action: edit).disabled(sending).accessibilityIdentifier(
                    "msg-edit")
            }
            if own {
                Button("chat.delete", role: .destructive, action: delete).disabled(sending)
                    .accessibilityIdentifier("msg-delete")
            }
        } label: {
            Image(systemName: "ellipsis")
                #if os(iOS)
                    // The bare glyph is 18 by 5 points, far below a touch target.
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
                #endif
        }
        .fixedSize()
        .accessibilityLabel(Text("msg.menu"))
        .accessibilityIdentifier(own ? "msg-menu" : "msg-received-menu")
    }
}

@MainActor
private struct ChatPhotoPreview: View {
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
            let peer = opening.source.peerId
            let policy = await feed.call(opening.context) { try $0.chatPhotoAuto(peerId: peer) }
            guard !Task.isCancelled, case .success(true) = policy else { return }
            let loaded = await loadChatPhoto(feed: feed, opening: opening)
            guard !Task.isCancelled else { return }
            original = loaded
        }
    }
}

private struct PhotoOpening: Identifiable {
    let id = UUID()
    let source: ChatForwardSource
    let context: ChatFeed.Context
}

private struct ChatPhotoViewer: View {
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
private func loadChatPhoto(feed: ChatFeed, opening: PhotoOpening) async -> Image? {
    let source = opening.source
    let result = await feed.call(opening.context) {
        try $0.chatPhotoOriginal(
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

private struct ChatRowID: Hashable {
    let group: Data
    let author: String
    let id: Data
}

extension ChatItem {
    fileprivate var rowID: ChatRowID {
        switch self {
        case .text(let message):
            return ChatRowID(group: message.groupId, author: message.author, id: message.id)

        case .photo(let photo):
            return ChatRowID(group: photo.groupId, author: photo.author, id: photo.id)

        case .sticker(let sticker):
            return ChatRowID(group: sticker.groupId, author: sticker.author, id: sticker.id)
        }
    }
}

extension ChatMessage {
    /// Its group and ID: a rotation keeps the ID under another group.
    var threadKey: Data { groupId + id }
}

/// The common native policy projects controls; send operations still authorize each action.
func chatWritable(_ state: ChatState?) -> Bool {
    chatComposerPolicy(state: state, text: "").writable
}

func chatSendable(_ state: ChatState?, _ text: String) -> Bool {
    chatComposerPolicy(state: state, text: text).sendable
}

/// Localized presentation of an authoritative native commitment, never inferred from history.
func chatSendStatus(
    _ result: Result<ChatSubmission, Error>, queued: String = "chat.queued",
    failed: String = "chat.sendFailed"
) -> ChatStatus? {
    switch result {
    case .success(.accepted):
        return nil

    case .success(.queued):
        return ChatStatus(key: queued, details: nil)

    case let .failure(error):
        return chatSendFailure(error, failed: failed)
    }
}

private func chatSendFailure(_ error: Error, failed: String) -> ChatStatus {
    let details = String(describing: error)
    if details.contains("notConnected") {
        return ChatStatus(key: "search.notConnected", details: details)
    }
    if details.contains("unpublished") {
        return ChatStatus(key: "chat.unpublished", details: details)
    }
    return ChatStatus(key: failed, details: details)
}

/// Node acceptance reads as Sent: only a device of the peer makes it Delivered, its receipt Read.
func chatMeta(_ message: ChatMessage) -> String {
    var parts: [String] = []
    if let time = chatTime(message) { parts.append(time) }
    if message.deleted { return parts.joined(separator: " | ") }
    if message.edited { parts.append(NSLocalizedString("chat.edited", comment: "")) }
    if let delivery = message.delivery {
        let key: String
        switch delivery {
        case _ where message.read:
            key = "chat.read"

        case .pending:
            key = "chat.pending"

        case .nodeAccepted:
            key = "chat.sent"

        case .deviceReceived:
            key = "chat.delivered"
        }
        parts.append(NSLocalizedString(key, comment: ""))
    }
    return parts.joined(separator: " | ")
}
