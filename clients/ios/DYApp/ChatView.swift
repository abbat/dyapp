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
    @State private var history: ChatHistory?
    @State private var status: ChatStatus?
    @State private var draft = ""
    @State private var sending = false
    /// The own message the composer changes, and the draft typed before it.
    @State private var editing: Data?
    @State private var held = ""
    @State private var deleting: ChatMessage?
    @State private var confirmClear = false
    @State private var confirmBlock = false
    @State private var renaming = false
    /// A clear, mute or block is running.
    @State private var busy = false

    private var messages: [ChatMessage] { history?.messages ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button("chats.title", action: back).accessibilityIdentifier("chat-back")
            header
            notice
            // Each dialog sits on its own view, so one never shadows another.
            thread.confirmationDialog("block.title", isPresented: $confirmBlock, titleVisibility: .visible) {
                Button("chat.block", role: .destructive, action: block)
            } message: {
                Text("block.text")
            }
            statusLines
            composer
        }
        .padding()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
        // The feed counts its reads, so a received message and a finished send both land here.
        .task(id: feed.version) { await reload() }
        .task(id: peer) { await feed.loadNames([peer]) }
        .sheet(isPresented: $renaming) {
            // The field starts from the local name: a public name is the peer's own claim.
            ChatRenameSheet(name: feed.names[peer].flatMap { $0.verified == nil ? $0.name : nil } ?? "") {
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
    }

    private var moreMenu: some View {
        Menu {
            Button("rename.title") { renaming = true }.accessibilityIdentifier("chat-rename")
            Button("chat.clear") { confirmClear = true }.accessibilityIdentifier("chat-clear")
            // A request offers its block on the notice instead.
            if history?.state != .requestReceived {
                Button("chat.block", role: .destructive) { confirmBlock = true }.accessibilityIdentifier("chat-block")
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
                    ForEach(messages, id: \.threadKey) { message in
                        ChatBubble(
                            message: message, own: message.author == feed.me,
                            writable: chatWritable(history?.state), sending: sending,
                            openProfile: openProfile,
                            edit: { startEditing(message) }, delete: { deleting = message }
                        )
                    }
                }
            }
            .onChange(of: messages.count) { _ in
                if let last = messages.last { proxy.scrollTo(last.threadKey, anchor: .bottom) }
            }
        }
        .accessibilityIdentifier("chat-thread")
    }

    @ViewBuilder private var statusLines: some View {
        if history?.state == .requestSent { Text("chat.firstWait").font(.caption) }
        if sending {
            Text("chat.sending").font(.caption).accessibilityIdentifier("chat-status")
        } else if let status {
            Text(LocalizedStringKey(status.key)).font(.caption).accessibilityIdentifier("chat-status")
            if let details = status.details {
                Text(verbatim: details).font(.caption).textSelection(.enabled)
                    .accessibilityIdentifier("chat-details")
            }
        }
        if chatTooLong(draft) { Text("chat.tooLong").font(.caption) }
        if editing != nil {
            HStack {
                Text("chat.editing").font(.caption).accessibilityIdentifier("chat-editing")
                Button("search.cancel", action: endEditing).accessibilityIdentifier("chat-edit-cancel")
            }
        }
    }

    // shortcut: one line of input, as multi-line TextField needs macOS 13 and iOS 16; use it when the targets rise.
    private var composer: some View {
        HStack {
            TextField("chat.placeholder", text: $draft)
                .disabled(!chatWritable(history?.state))
                .onSubmit(send)
                .accessibilityIdentifier("chat-input")
            Button("chat.send", action: send)
                .disabled(sending || !chatSendable(history?.state, draft))
                .accessibilityIdentifier("chat-send")
        }
    }

    private func reload() async {
        let peer = peer
        let result = await feed.call { try $0.chatHistory(peerId: peer) }
        if Task.isCancelled { return }
        switch result {
        case let .success(loaded):
            if history == nil { status = nil }
            history = loaded
            await feed.markRead(peer)

        case let .failure(error):
            if history == nil { status = ChatStatus(key: "chat.loadFailed", details: String(describing: error)) }
        }
    }

    private func send() {
        let text = draft
        guard !sending, chatSendable(history?.state, text) else { return }
        let target = messages.first { $0.threadKey == editing }
        // An edit whose message is gone must not go out as a new text.
        if editing != nil, target == nil { return }
        let before = messages
        sending = true
        status = nil
        Task {
            let outcome: ChatStatus?
            if let target {
                outcome = await feed.edit(target, text: text, peer: peer)
            } else {
                outcome = await feed.send(text, to: peer, after: before)
            }
            sending = false
            status = outcome
            guard outcome?.details == nil else { return }
            // The field stays editable during a send, so text typed meanwhile is kept.
            if target != nil { endEditing() } else if draft == text { draft = "" }
        }
    }

    private func startEditing(_ message: ChatMessage) {
        if editing == nil { held = draft }
        draft = message.text ?? ""
        editing = message.threadKey
        status = nil
    }

    private func endEditing() {
        draft = held
        held = ""
        editing = nil
    }

    private func delete(_ message: ChatMessage) {
        Task {
            status = await feed.delete(message, peer: peer)
            // A deleted message has nothing left to edit.
            if status?.details == nil, editing == message.threadKey { endEditing() }
        }
    }

    private func clear() {
        busy = true
        Task {
            let failed = await feed.clear(peer)
            status = failed ?? ChatStatus(key: "chat.cleared", details: nil)
            if failed == nil, editing != nil { endEditing() }
            busy = false
        }
    }

    private func setMuted(_ muted: Bool) {
        busy = true
        Task {
            status = await feed.setMuted(muted, peer: peer)
            // The menu stays off until its label can tell the new state.
            await reload()
            busy = false
        }
    }

    private func block() {
        busy = true
        Task {
            status = await feed.block(peer)
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

    var body: some View {
        HStack {
            if own { Spacer(minLength: 48) }
            if own, !message.deleted { menu }
            VStack(alignment: .leading, spacing: 4) {
                if let shared = sharedProfile(message) {
                    // It names only the short Peer ID: no message fetches anything before the press.
                    Button { openProfile(shared) } label: {
                        Label(chatPreview(message), systemImage: "person.crop.circle")
                    }
                    .accessibilityIdentifier("msg-profile")
                } else {
                    Text(verbatim: chatPreview(message)).textSelection(.enabled)
                }
                let meta = chatMeta(message)
                if !meta.isEmpty { Text(verbatim: meta).font(.caption) }
            }
            .padding(8)
            .background(RoundedRectangle(cornerRadius: 12).fill(
                own ? Color.accentColor.opacity(0.2) : Color.secondary.opacity(0.15)
            ))
            if !own { Spacer(minLength: 48) }
        }
    }

    // A button rather than a context menu: right click on selectable text opens the text's own menu.
    private var menu: some View {
        Menu {
            if writable {
                Button("chat.edit", action: edit).disabled(sending).accessibilityIdentifier("msg-edit")
            }
            Button("chat.delete", role: .destructive, action: delete).accessibilityIdentifier("msg-delete")
        } label: {
            Image(systemName: "ellipsis")
        }
        .fixedSize()
        .accessibilityLabel(Text("msg.menu"))
        .accessibilityIdentifier("msg-menu")
    }
}

extension ChatMessage {
    /// Its group and ID: a rotation keeps the ID under another group.
    var threadKey: Data { groupId + id }
}

/// A shown chat takes text unless the first message waits for a reply or a match ended; nil is still loading.
func chatWritable(_ state: ChatState?) -> Bool {
    guard let state else { return false }
    return state != .requestSent && state != .closed
}

/// As the core's limit.
func chatTooLong(_ text: String) -> Bool { text.utf8.count > 65_536 }

func chatSendable(_ state: ChatState?, _ text: String) -> Bool {
    chatWritable(state) && !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !chatTooLong(text)
}

/// Whether a failed send still stored its text: another copy from the same draft must not go out.
func stagedNew(before: [ChatMessage], after: [ChatMessage], me: String?) -> Bool {
    let known = Set(before.map(\.id))
    return after.contains { $0.author == me && !known.contains($0.id) }
}

/// A failure's line; a change that was `staged` reads as `queued` instead, and keeps no details.
func chatSendStatus(
    _ error: Error, staged: Bool, queued: String = "chat.queued", failed: String = "chat.sendFailed"
) -> ChatStatus {
    let details = String(describing: error)
    if staged { return ChatStatus(key: queued, details: nil) }
    if details.contains("notConnected") { return ChatStatus(key: "search.notConnected", details: details) }
    if details.contains("unpublished") { return ChatStatus(key: "chat.unpublished", details: details) }
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
