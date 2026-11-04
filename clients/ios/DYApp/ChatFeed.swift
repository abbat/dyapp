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
        }
        self.identity = identity
        #if DEBUG
        if ChatTestPeer.enabled, !testPeerStarted {
            testPeerStarted = true
            ChatTestPeer.start(me: identity.peerId)
        }
        #endif
        await read()
        while !Task.isCancelled {
            _ = await call { try $0.receive() }
            await read()
            try? await Task.sleep(nanoseconds: Self.receiveIntervalNs)
        }
    }

    /// Runs a blocking chat call off the main actor.
    func call<T>(_ action: @escaping @Sendable (ChatSession) throws -> T) async -> Result<T, Error> {
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
    }
}
