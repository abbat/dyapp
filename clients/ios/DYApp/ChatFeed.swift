import Foundation
import SwiftUI

private struct ChatOutcome<T>: @unchecked Sendable {
    let result: Result<T, Error>
}

/// This identity's chat rows. ContentView owns them, as the lists and the mailbox poll outlive the
/// pages opened over them.
@MainActor
final class ChatFeed: ObservableObject {
    struct Context: Sendable {
        let owner: UUID
        let identity: ChatIdentity
    }

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
    @Published private(set) var owner = UUID()
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

    var context: Context? { identity.map { Context(owner: owner, identity: $0) } }

    func isCurrent(_ context: Context) -> Bool { context.owner == owner && identity != nil }

    /// Invalidates presentation callbacks without interrupting an already owned native operation.
    func deactivate() {
        owner = UUID()
        identity = nil
        rows = []
        liked = []
        likedOpen = false
        requestsOpen = false
        likedProfiles = nil
        blocked = nil
        names = [:]
        photos.reset()
    }

    @discardableResult
    func activate(_ identity: ChatIdentity) -> Context {
        if self.identity?.peerId != identity.peerId
            || self.identity?.candidate.location.marker != identity.candidate.location.marker
        {
            deactivate()
        }
        self.identity = identity
        return Context(owner: owner, identity: identity)
    }

    /// Reads the rows, then receives and reads again until cancelled. Another identity starts over,
    /// so a switch never shows the previous identity's chats.
    func poll(_ identity: ChatIdentity) async {
        let context = activate(identity)
        await photos.use(identity)
        guard isCurrent(context), !Task.isCancelled else { return }
        #if DEBUG
            if ChatTestPeer.enabled, !testPeerStarted {
                testPeerStarted = true
                ChatTestPeer.start(me: identity.peerId)
            }
        #endif
        await read()
        while !Task.isCancelled && isCurrent(context) {
            #if os(macOS)
                let (rowsBefore, likedBefore) = (rows, liked)
            #endif
            _ = await call(context) { try $0.receive() }
            guard !Task.isCancelled, isCurrent(context) else { return }
            await read()
            #if os(macOS)
                // After a switch `rows` are the next identity's and would count as new.
                if !Task.isCancelled, isCurrent(context) {
                    await noticeNewMail(before: rowsBefore, likedBefore: likedBefore)
                }
            #endif
            try? await Task.sleep(nanoseconds: Self.receiveIntervalNs)
        }
    }

    /// Runs a blocking chat call off the main actor.
    func call<T>(_ action: @escaping @Sendable (ChatSession) throws -> T) async -> Result<T, Error>
    {
        guard let context else { return .failure(IdentitySlotError.peerIDMismatch) }
        return await call(context, action)
    }

    /// Capture at the UI action, before scheduling; a successor owner cannot retarget this call.
    func call<T>(
        _ context: Context, _ action: @escaping @Sendable (ChatSession) throws -> T
    ) async -> Result<T, Error> {
        guard isCurrent(context) else { return .failure(IdentitySlotError.peerIDMismatch) }
        let identity = context.identity
        return await Task.detached(priority: .userInitiated) {
            ChatOutcome(result: Result { try RustBridge.withChats(identity, action) })
        }.value.result
    }

    /// Failures stay quiet and keep the last rows: no network and an unpublished profile are normal.
    func read() async {
        guard let context else { return }
        let result = await call(context) { (try $0.chatRows(), try $0.likedYou()) }
        guard isCurrent(context), case let .success((rows, liked)) = result else { return }
        self.rows = rows
        self.liked = liked
        version += 1
    }

    func submit(_ text: String, to peer: String, in context: Context) async -> Result<
        ChatSubmission, Error
    > {
        let sent = await call(context) { try $0.submitText(peerId: peer, text: text) }
        if isCurrent(context) { await read() }
        return sent
    }

    /// Replaces the own message and preserves the native commitment independently of UI lifetime.
    func edit(_ message: ChatMessage, text: String, peer: String, in context: Context) async
        -> Result<ChatSubmission, Error>
    {
        let (group, id) = (message.groupId, message.id)
        let changed = await call(context) {
            try $0.submitEditText(peerId: peer, groupId: group, messageId: id, text: text)
        }
        if isCurrent(context) { await read() }
        return changed
    }

    /// Deletes the own `message` here and asks the peer's app to delete it too.
    func delete(_ message: ChatMessage, peer: String, in context: Context) async
        -> Result<ChatSubmission, Error>
    {
        let (group, id) = (message.groupId, message.id)
        let changed = await call(context) {
            try $0.submitDeleteText(peerId: peer, groupId: group, messageId: id)
        }
        if isCurrent(context) { await read() }
        return changed
    }

    /// Clears this device's copy of the chat; nil when it held, else the line to show.
    func clear(_ peer: String, in context: Context? = nil) async -> ChatStatus? {
        guard let context = context ?? self.context else {
            return ChatStatus(
                key: "chat.clearFailed",
                details: String(describing: IdentitySlotError.peerIDMismatch))
        }
        let cleared = await call(context) { try $0.clearChat(peerId: peer) }
        if isCurrent(context) { await read() }
        guard case let .failure(error) = cleared else { return nil }
        return ChatStatus(key: "chat.clearFailed", details: String(describing: error))
    }

    func setMuted(_ muted: Bool, peer: String, in context: Context? = nil) async -> ChatStatus? {
        guard let context = context ?? self.context else {
            return ChatStatus(
                key: "chat.muteFailed",
                details: String(describing: IdentitySlotError.peerIDMismatch))
        }
        let changed = await call(context) { try $0.setMuted(peerId: peer, muted: muted) }
        if isCurrent(context) { await read() }
        guard case let .failure(error) = changed else { return nil }
        return ChatStatus(key: "chat.muteFailed", details: String(describing: error))
    }

    /// Blocks `peer` and keeps the outcome for Chats; nil when it held, else the line to show.
    func block(_ peer: String, in context: Context? = nil) async -> ChatStatus? {
        guard let context = context ?? self.context else {
            return ChatStatus(
                key: "chat.blockFailed",
                details: String(describing: IdentitySlotError.peerIDMismatch))
        }
        let result = await call(context) { try $0.block(peerId: peer) }
        if isCurrent(context) { await read() }
        switch result {
        case let .success(block):
            if isCurrent(context) { blocked = block }
            return nil

        case let .failure(error):
            return ChatStatus(key: "chat.blockFailed", details: String(describing: error))
        }
    }

    /// Reads the names, then again with the public ones a fetch brings, then the main thumbnails.
    /// A failed fetch keeps what shows, and the next view tries again.
    func loadNames(_ peers: [String]) async {
        guard !peers.isEmpty, let context else { return }
        await readNames(peers)
        guard isCurrent(context), let worker = await photos.use(context.identity),
            isCurrent(context)
        else { return }
        await IdentityIO.performBlocking { _ = try? worker.searcher.profiles(peerIds: peers) }
        guard isCurrent(context) else { return }
        await readNames(peers)
        guard isCurrent(context) else { return }
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
        guard let context else { return }
        let identity = context.identity
        guard let worker = await photos.use(identity), isCurrent(context) else { return }
        let read = await IdentityIO.performBlocking {
            ChatOutcome(
                result: Result {
                    try RustBridge.contactNames(identity, searcher: worker.searcher, peers: peers)
                })
        }.result
        guard isCurrent(context), case let .success(views) = read else {
            return
        }
        for view in views { names[view.peerId] = view }
    }

    /// Marks an open chat read and tells the peer; a read mark that fails to go out waits for the next one.
    func markRead(_ peer: String, in context: Context) async {
        let marked = await call(context) { session -> UInt32 in
            let marked = try session.markRead(peerId: peer)
            _ = try? session.sendReads(peerId: peer)
            return marked
        }
        if isCurrent(context), case let .success(count) = marked, count > 0 { await read() }
    }

    /// Liked you profiles come from the network, so only an open list fetches them.
    func fetchLiked() async {
        guard likedOpen, !liked.isEmpty, let context else { return }
        let peers = liked
        let result = await call(context) { try $0.profiles(peerIds: peers) }
        guard !Task.isCancelled, isCurrent(context) else { return }
        likedProfiles = result
        if case let .success(found) = result {
            await photos.fetch(found.compactMap { $0.photos.first }, full: false)
        }
    }
}
