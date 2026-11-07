import Foundation
import SwiftUI

private struct ChatOutcome<T>: @unchecked Sendable {
    let result: Result<T, Error>
}

/// This identity's chat rows. ContentView owns them, as the lists and the mailbox poll outlive the
/// pages opened over them.
@MainActor
final class ChatFeed: ObservableObject {
    private static var receiveIntervalNs: UInt64 {
        #if DEBUG
            if ChatTestPeer.enabled { return 5_000_000_000 }
        #endif
        return 60_000_000_000
    }

    @Published private(set) var rows: [ChatRow] = []
    /// The Liked you peers, read locally with the rows.
    @Published private(set) var liked: [String] = []
    /// Counts the reads, so an open chat reloads for a change the rows do not show.
    @Published private(set) var version = 0
    @Published var likedOpen = false
    @Published var requestsOpen = false
    /// The last fetch of the Liked you profiles; nil until the first one ends.
    @Published private(set) var likedProfiles: Result<[FoundProfile], Error>?
    /// The last block's outcome, which Chats shows.
    @Published private(set) var blocked: ChatBlock?
    /// How the screens name each peer, as far as read.
    @Published private(set) var names: [String: ContactNameView] = [:]
    /// Holds the fetched profiles that the public names and photos come from.
    let photos = PeerPhotos()
    private var identity: ChatIdentity?
    #if DEBUG
        private var testPeerStarted = false
    #endif

    var me: String? { identity?.peerId }

    /// Reads the rows, then receives and reads again until cancelled. Another identity starts over,
    /// so a switch never shows the previous identity's chats.
    func poll(_ identity: ChatIdentity) async {
        if self.identity?.peerId != identity.peerId {
            rows = []
            liked = []
            likedOpen = false
            requestsOpen = false
            likedProfiles = nil
            blocked = nil
            names = [:]
        }
        self.identity = identity
        photos.use(identity)
        #if DEBUG
            if ChatTestPeer.enabled, !testPeerStarted {
                testPeerStarted = true
                ChatTestPeer.start(me: identity.peerId)
            }
        #endif
        await read()
        while !Task.isCancelled {
            #if os(macOS)
                let (rowsBefore, likedBefore) = (rows, liked)
            #endif
            _ = await call { try $0.receive() }
            await read()
            #if os(macOS)
                // After a switch `rows` are the next identity's and would count as new.
                if !Task.isCancelled {
                    await noticeNewMail(before: rowsBefore, likedBefore: likedBefore)
                }
            #endif
            try? await Task.sleep(nanoseconds: Self.receiveIntervalNs)
        }
    }

    /// Runs a blocking chat call off the main actor.
    func call<T>(_ action: @escaping @Sendable (ChatSession) throws -> T) async -> Result<T, Error>
    {
        guard let identity else { return .failure(IdentitySlotError.peerIDMismatch) }
        return await Task.detached(priority: .userInitiated) {
            ChatOutcome(result: Result { try RustBridge.withChats(identity, action) })
        }.value.result
    }

    /// Failures stay quiet and keep the last rows: no network and an unpublished profile are normal.
    func read() async {
        let reading = identity?.peerId
        let result = await call { (try $0.chatRows(), try $0.likedYou()) }
        guard reading == identity?.peerId, case let .success((rows, liked)) = result else { return }
        self.rows = rows
        self.liked = liked
        version += 1
    }

    /// Stores and sends `text`; nil when it went out, else the line to show.
    func send(_ text: String, to peer: String, after before: [ChatMessage]) async -> ChatStatus? {
        let sent = await call { try $0.sendText(peerId: peer, text: text) }
        await read()
        guard case let .failure(error) = sent else { return nil }
        let after = (try? await call { try $0.chatHistory(peerId: peer) }.get())?.messages ?? before
        return chatSendStatus(error, staged: stagedNew(before: before, after: after, me: me))
    }

    /// Replaces the text of the own `message`; nil when a node took it, else the line to show.
    func edit(_ message: ChatMessage, text: String, peer: String) async -> ChatStatus? {
        let (group, id) = (message.groupId, message.id)
        let changed = await call {
            try $0.editText(peerId: peer, groupId: group, messageId: id, text: text)
        }
        await read()
        guard case let .failure(error) = changed else { return nil }
        let stored = await shown(message, peer: peer)
        let staged = stored.map { $0.edited && !$0.deleted && $0.text == text } ?? false
        return chatSendStatus(
            error, staged: staged, queued: "chat.editQueued", failed: "chat.editFailed")
    }

    /// Deletes the own `message` here and asks the peer's app to delete it too.
    func delete(_ message: ChatMessage, peer: String) async -> ChatStatus? {
        let (group, id) = (message.groupId, message.id)
        let changed = await call { try $0.deleteText(peerId: peer, groupId: group, messageId: id) }
        await read()
        guard case let .failure(error) = changed else { return nil }
        let staged = await shown(message, peer: peer)?.deleted ?? false
        return chatSendStatus(
            error, staged: staged, queued: "chat.deleteQueued", failed: "chat.deleteFailed")
    }

    /// Clears this device's copy of the chat; nil when it held, else the line to show.
    func clear(_ peer: String) async -> ChatStatus? {
        let cleared = await call { try $0.clearChat(peerId: peer) }
        await read()
        guard case let .failure(error) = cleared else { return nil }
        return ChatStatus(key: "chat.clearFailed", details: String(describing: error))
    }

    func setMuted(_ muted: Bool, peer: String) async -> ChatStatus? {
        let changed = await call { try $0.setMuted(peerId: peer, muted: muted) }
        await read()
        guard case let .failure(error) = changed else { return nil }
        return ChatStatus(key: "chat.muteFailed", details: String(describing: error))
    }

    /// Blocks `peer` and keeps the outcome for Chats; nil when it held, else the line to show.
    func block(_ peer: String) async -> ChatStatus? {
        let result = await call { try $0.block(peerId: peer) }
        await read()
        switch result {
        case let .success(block):
            blocked = block
            return nil

        case let .failure(error):
            return ChatStatus(key: "chat.blockFailed", details: String(describing: error))
        }
    }

    /// Reads the names, then again with the public ones a fetch brings, then the main thumbnails.
    /// A failed fetch keeps what shows, and the next view tries again.
    func loadNames(_ peers: [String]) async {
        guard !peers.isEmpty else { return }
        await readNames(peers)
        guard let searcher = photos.searcher else { return }
        await Task.detached(priority: .utility) { _ = try? searcher.profiles(peerIds: peers) }.value
        await readNames(peers)
        await photos.fetch(peers.compactMap { names[$0]?.photo }, full: false)
    }

    /// Saves the name on this device; nil when it held, else the line to show.
    func rename(_ peer: String, name: String) async -> ChatStatus? {
        let renamed = await call { try $0.setContactName(peerId: peer, name: name) }
        guard case let .failure(error) = renamed else {
            await readNames([peer])
            return nil
        }
        let details = String(describing: error)
        if details.contains("invalidName") {
            return ChatStatus(key: "rename.invalid", details: nil)
        }
        return ChatStatus(key: "rename.failed", details: details)
    }

    /// A failed read keeps the last names.
    private func readNames(_ peers: [String]) async {
        guard let identity else { return }
        photos.use(identity)
        guard let searcher = photos.searcher else { return }
        let read = await Task.detached(priority: .userInitiated) {
            ChatOutcome(
                result: Result {
                    try RustBridge.contactNames(identity, searcher: searcher, peers: peers)
                })
        }.value.result
        guard identity.peerId == self.identity?.peerId, case let .success(views) = read else {
            return
        }
        for view in views { names[view.peerId] = view }
    }

    /// A failed delivery can follow a stored change, so the failure reads the message again.
    private func shown(_ message: ChatMessage, peer: String) async -> ChatMessage? {
        let messages = (try? await call { try $0.chatHistory(peerId: peer) }.get())?.messages
        return messages?.first { $0.threadKey == message.threadKey }
    }

    /// Marks an open chat read and tells the peer; a read mark that fails to go out waits for the next one.
    func markRead(_ peer: String) async {
        let marked = await call { session -> UInt32 in
            let marked = try session.markRead(peerId: peer)
            _ = try? session.sendReads(peerId: peer)
            return marked
        }
        if case let .success(count) = marked, count > 0 { await read() }
    }

    /// Liked you profiles come from the network, so only an open list fetches them.
    func fetchLiked() async {
        guard likedOpen, !liked.isEmpty else { return }
        let peers = liked
        let result = await call { try $0.profiles(peerIds: peers) }
        if !Task.isCancelled { likedProfiles = result }
        if case let .success(found) = result {
            await photos.fetch(found.compactMap { $0.photos.first }, full: false)
        }
    }
}
