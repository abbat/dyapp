import SwiftUI

/// The match card, the like and message buttons and what the last action told.
struct ProfileLikesBlock: View {
    @ObservedObject var page: ProfilePage
    let message: () -> Void
    @State private var confirmUnlike = false

    private var ready: Bool { page.likes != nil && !page.sending }

    var body: some View {
        let likes = page.likes
        if likes?.matched == true {
            VStack(alignment: .leading) {
                Text("view.match").font(.headline)
                Text("view.matchText").font(.caption)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("view-match")
        }
        if likes?.ours == true {
            Button("view.liked") { confirmUnlike = true }
                .disabled(!ready)
                .accessibilityIdentifier("view-unlike")
                .confirmationDialog("unlike.title", isPresented: $confirmUnlike) {
                    Button("unlike.action", role: .destructive) { Task { await page.like(false) } }
                } message: {
                    Text("unlike.text")
                }
        }
        Button("view.message", action: message)
            .accessibilityIdentifier(likes?.matched == true ? "view-message-match" : "view-message")
        if likes?.ours != true, likes?.passed != true {
            Button("view.like") { Task { await page.like(true) } }
                .disabled(!ready)
                .accessibilityIdentifier("view-like")
        }
        if likes?.matched != true { Text("view.firstHint").font(.caption) }
        if likes?.ours == true { Text("view.unlikeHint").font(.caption) }
        ProfileActionStatus(page: page)
    }
}

struct ProfileActionStatus: View {
    @ObservedObject var page: ProfilePage

    @ViewBuilder var body: some View {
        if page.sending {
            Text("view.sending").font(.caption).accessibilityIdentifier("view-status")
        } else if let status = page.status ?? page.likesFailure {
            Text(LocalizedStringKey(status.key)).font(.caption).accessibilityIdentifier(
                "view-status")
            if let details = status.details {
                Text(verbatim: details).font(.caption).textSelection(.enabled)
                    .accessibilityIdentifier("view-details")
            }
        }
    }
}

/// Sends this page's link to an open chat; what it told shows with the like's status above.
struct ProfileShareBlock: View {
    @ObservedObject var page: ProfilePage
    @ObservedObject var feed: ChatFeed
    let openChat: (String) -> Void
    @State private var open = false
    @State private var opening = UUID()
    @State private var sentTo: String?

    private var targets: [ChatRow] { profileShareTargets(feed.rows, sharedPeer: page.peer) }

    var body: some View {
        let expected = opening
        let pageOpening = page.opening
        Button("view.share") {
            guard page.current(pageOpening), opening == expected, !page.sending else { return }
            open.toggle()
            opening = UUID()
            sentTo = nil
        }
        .disabled(page.sending)
        .accessibilityIdentifier("view-share")
        // A page opened from search comes before Chats has read any name.
        .task(id: open) { if open { await feed.loadNames(targets.map(\.peerId)) } }
        if open {
            if let sentTo {
                Button("view.message") {
                    guard open, opening == expected, self.sentTo == sentTo,
                        page.current(pageOpening), !page.sending
                    else { return }
                    openChat(sentTo)
                }
                .accessibilityIdentifier("share-open-chat")
            } else if targets.isEmpty {
                Text("view.shareNone").font(.caption).accessibilityIdentifier("view-share-none")
            } else {
                ForEach(Array(targets.enumerated()), id: \.element.peerId) { index, row in
                    Button {
                        Task {
                            guard open, opening == expected else { return }
                            let committed = await page.share(to: row.peerId, opening: pageOpening)
                            guard open, opening == expected else { return }
                            if committed { sentTo = row.peerId }
                        }
                    } label: {
                        Text(verbatim: contactLabel(row.peerId, feed.names[row.peerId]))
                    }
                    .disabled(page.sending)
                    .accessibilityIdentifier("share-\(index + 1)")
                }
            }
        }
    }
}

/// Pass and block sit below the profile, away from the like.
struct ProfileQuietActions: View {
    @ObservedObject var page: ProfilePage
    let blocked: () -> Void
    @State private var confirmBlock = false

    private var ready: Bool { page.likes != nil && !page.sending }

    var body: some View {
        if page.likes?.passed == true {
            Button("view.unpass") { Task { await page.pass(false) } }
                .disabled(!ready)
                .accessibilityIdentifier("view-unpass")
        } else if page.likes?.ours == false {
            Button("view.pass") { Task { await page.pass(true) } }
                .disabled(!ready)
                .accessibilityIdentifier("view-pass")
        }
        Button("chat.block", role: .destructive) { confirmBlock = true }
            .disabled(page.sending)
            .accessibilityIdentifier("view-block")
            .confirmationDialog("block.title", isPresented: $confirmBlock) {
                Button("chat.block", role: .destructive) {
                    Task { if await page.block() { blocked() } }
                }
            } message: {
                Text("block.text")
            }
    }
}
