import SwiftUI

/// Liked you and message requests with counts from local data, then the conversations.
struct ChatsView: View {
    @ObservedObject var feed: ChatFeed
    // Swapped in by state: a navigation link in the macOS 12 detail column would leave no way back.
    @State private var opened: OpenedProfile?
    @State private var chat: String?

    private var requests: [ChatRow] { feed.rows.filter { $0.state == .requestReceived } }
    private var conversations: [ChatRow] { feed.rows.filter { $0.state != .requestReceived } }

    var body: some View {
        Group {
            // The page goes over the chat that opened it, and its back leads there.
            if let opened {
                FoundProfileView(
                    peer: opened.peer, known: opened.known, feed: feed,
                    back: { self.opened = nil },
                    message: {
                        chat = opened.peer
                        self.opened = nil
                    },
                    blocked: {
                        chat = nil
                        self.opened = nil
                    }
                )
                .id(opened.peer)
            } else if let chat {
                ChatView(
                    feed: feed, peer: chat,
                    profile: { opened = OpenedProfile(peer: chat, known: nil) },
                    back: { self.chat = nil },
                    blocked: { self.chat = nil }
                )
            } else {
                list
            }
        }
        .task(id: feed.likedOpen ? feed.liked : nil) { await feed.fetchLiked() }
    }

    private var list: some View {
        Form {
            if let block = feed.blocked { BlockedNote(block: block) }
            ChatsExpandRow(
                id: "chats-liked", title: "chats.liked", text: "chats.likedText",
                count: feed.liked.count, open: $feed.likedOpen
            )
            if feed.likedOpen {
                LikedList(liked: feed.liked, fetch: feed.likedProfiles) {
                    opened = OpenedProfile(peer: $0.peerId, known: $0)
                }
            }
            ChatsExpandRow(
                id: "chats-requests", title: "chats.requests", text: "chats.requestsText",
                count: requests.count, open: $feed.requestsOpen
            )
            if feed.requestsOpen {
                if requests.isEmpty {
                    Text("chats.noRequests").font(.caption).accessibilityIdentifier("chats-no-requests")
                }
                rows(requests, prefix: "request")
            }
            Text("chats.conversations").font(.headline)
            if conversations.isEmpty {
                Text("chats.empty").accessibilityIdentifier("chats-empty")
            }
            rows(conversations, prefix: "chat")
            Text("chats.identity").font(.caption).accessibilityIdentifier("chats-identity")
        }
    }

    private func rows(_ rows: [ChatRow], prefix: String) -> some View {
        ForEach(Array(rows.enumerated()), id: \.element.peerId) { index, row in
            Button {
                chat = row.peerId
            } label: {
                ChatListRow(row: row)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("\(prefix)-\(index + 1)")
        }
    }
}

/// A peer's page to show; a chat knows only the peer, so the page fetches the profile.
private struct OpenedProfile {
    let peer: String
    let known: FoundProfile?
}

/// The last block's outcome: a pending revocation says why; the block itself holds either way.
private struct BlockedNote: View {
    let block: ChatBlock

    private var text: LocalizedStringKey {
        switch block.revocation {
        case .notNeeded:
            return "chats.blockedNotNeeded"

        case .pending:
            return "chats.blockedPending"

        case .confirmed:
            return "chats.blockedConfirmed"
        }
    }

    var body: some View {
        Text(text).font(.caption).accessibilityIdentifier("chats-blocked-note")
        if block.revocation == .pending, let detail = block.detail {
            Text(verbatim: detail).font(.caption).textSelection(.enabled)
                .accessibilityIdentifier("chats-blocked-details")
        }
    }
}

/// A row that opens or closes the list below it and tells how many it holds.
private struct ChatsExpandRow: View {
    let id: String
    let title: LocalizedStringKey
    let text: LocalizedStringKey
    let count: Int
    @Binding var open: Bool

    var body: some View {
        Button {
            open.toggle()
        } label: {
            HStack {
                VStack(alignment: .leading) {
                    Text(title).font(.headline)
                    Text(text).font(.caption)
                }
                Spacer()
                Text(verbatim: String(count))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(Text(verbatim: String(count)))
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityIdentifier(id)
    }
}

/// Closing and opening the list fetches again, so a failure needs no retry button of its own.
private struct LikedList: View {
    let liked: [String]
    let fetch: Result<[FoundProfile], Error>?
    let open: (FoundProfile) -> Void

    var body: some View {
        if liked.isEmpty {
            Text("chats.none").font(.caption).accessibilityIdentifier("chats-none")
        } else if let fetch {
            switch fetch {
            case let .success(found):
                profiles(found)

            case let .failure(error):
                Text(LocalizedStringKey(profileLoadKey(error))).accessibilityIdentifier("chats-liked-status")
                Text(verbatim: String(describing: error)).font(.caption).textSelection(.enabled)
                    .accessibilityIdentifier("chats-liked-details")
            }
        } else {
            Text("view.loading").font(.caption)
        }
    }

    /// The fetch can be older than the IDs, so a peer who left the list leaves at once.
    @ViewBuilder
    private func profiles(_ found: [FoundProfile]) -> some View {
        let shown = found.filter { liked.contains($0.peerId) }
        ForEach(Array(shown.enumerated()), id: \.element.peerId) { index, profile in
            Button {
                open(profile)
            } label: {
                SearchResultRow(found: profile)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("liked-\(index + 1)")
        }
        if shown.count < liked.count { Text("chats.likedMissing").font(.caption) }
    }
}

private struct ChatListRow: View {
    let row: ChatRow

    var body: some View {
        HStack {
            VStack(alignment: .leading) {
                Text(verbatim: "@\(row.peerId.suffix(6))").font(.headline)
                Text(verbatim: chatPreview(row.lastMessage)).lineLimit(1)
            }
            Spacer()
            VStack(alignment: .trailing) {
                if let time = chatTime(row.lastMessage) { Text(verbatim: time).font(.caption) }
                if row.unreadCount > 0 {
                    Text(verbatim: String(row.unreadCount)).accessibilityLabel(Text(verbatim: String(
                        format: NSLocalizedString("chats.unread", comment: ""), String(row.unreadCount)
                    )))
                }
                if row.muted { Text("chats.muted").font(.caption).accessibilityIdentifier("chat-muted") }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The status of a failed profile fetch.
func profileLoadKey(_ error: Error) -> String {
    String(describing: error).contains("notConnected") ? "search.notConnected" : "view.loadFailed"
}

func chatPreview(_ message: ChatMessage?) -> String {
    guard let message else { return NSLocalizedString("chats.noMessages", comment: "") }
    if message.deleted { return NSLocalizedString("chats.deleted", comment: "") }
    if message.forwarded {
        return String(format: NSLocalizedString("chats.forwarded", comment: ""), message.text ?? "")
    }
    return message.text ?? ""
}

func chatTime(_ message: ChatMessage?) -> String? {
    guard let sent = message?.sentAtMs else { return nil }
    return Date(timeIntervalSince1970: Double(sent) / 1_000).formatted(date: .numeric, time: .shortened)
}
