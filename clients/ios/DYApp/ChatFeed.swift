import Foundation
import SwiftUI

private struct ChatOutcome<T>: @unchecked Sendable {
    let result: Result<T, Error>
}

/// This identity's chat rows. ContentView owns them, as the lists and the mailbox poll outlive the
/// pages opened over them.
@MainActor
final class ChatFeed: ObservableObject {
    private static let receiveIntervalNs: UInt64 = 60_000_000_000

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

    /// Liked you profiles come from the network, so only an open list fetches them.
    func fetchLiked() async {
        guard likedOpen, !liked.isEmpty else { return }
        let peers = liked
        let result = await call { try $0.profiles(peerIds: peers) }
        if !Task.isCancelled { likedProfiles = result }
    }
}
