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
    let back: () -> Void
    @State private var history: ChatHistory?
    @State private var status: ChatStatus?
    @State private var draft = ""
    @State private var sending = false

    private var messages: [ChatMessage] { history?.messages ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button("chats.title", action: back).accessibilityIdentifier("chat-back")
            Text(verbatim: "@\(peer.suffix(6))").font(.headline)
            notice
            thread
            statusLines
            composer
        }
        .padding()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat")
        // The feed counts its reads, so a received message and a finished send both land here.
        .task(id: feed.version) { await reload() }
    }

    @ViewBuilder private var notice: some View {
        switch history?.state {
        case ChatState.none?, .requestSent?:
            Text("chat.firstNotice").font(.caption).accessibilityIdentifier("chat-first-notice")

        case .requestReceived?:
            Text("chat.requestNotice").font(.caption).accessibilityIdentifier("chat-request-notice")

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
                        ChatBubble(message: message, own: message.author == feed.me)
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
        let before = messages
        sending = true
        status = nil
        Task {
            let outcome = await feed.send(text, to: peer, after: before)
            sending = false
            status = outcome
            // The field stays editable during a send, so text typed meanwhile is kept.
            if outcome?.details == nil, draft == text { draft = "" }
        }
    }
}

private struct ChatBubble: View {
    let message: ChatMessage
    let own: Bool

    var body: some View {
        HStack {
            if own { Spacer(minLength: 48) }
            VStack(alignment: .leading, spacing: 4) {
                Text(verbatim: chatPreview(message)).textSelection(.enabled)
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

func chatSendStatus(_ error: Error, staged: Bool) -> ChatStatus {
    let details = String(describing: error)
    if staged { return ChatStatus(key: "chat.queued", details: nil) }
    if details.contains("notConnected") { return ChatStatus(key: "search.notConnected", details: details) }
    if details.contains("unpublished") { return ChatStatus(key: "chat.unpublished", details: details) }
    return ChatStatus(key: "chat.sendFailed", details: details)
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
