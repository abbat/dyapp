import Foundation

/// The identity whose chats a screen shows.
struct ChatIdentity: Sendable {
    let candidate: IdentityCandidate
    let peerId: String
}

extension RustBridge {
    /// One session at a time: two would advance the same saved MLS state and the later one would
    /// drop the other's commit. Removing an identity waits too, so a receive cannot write into its folder.
    static let chatsLock = NSLock()

    /// Runs a blocking `action` on this identity's chats; call it off the main thread.
    static func withChats<T>(_ identity: ChatIdentity, _ action: (ChatSession) throws -> T) throws
        -> T
    {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        let location = identity.candidate.location
        // Core bootstraps before admitting the shared lease; never wrap this call in another lease.
        let session = try identityStore().registry.openChats(
            id: location.marker, expectedPeer: identity.peerId,
            store: keyStore(for: location).requiringExistingKey())
        return try withExtendedLifetime(session) { try action(session) }
    }

    /// The native probe must first establish authority for the original slot and device.
    static func completeRevokedDevice(_ identity: ChatIdentity) -> (removed: Bool, detail: String?)
    {
        do {
            let location = identity.candidate.location
            let directory = try MobileDeviceDirectory(
                registry: identityStore().registry, id: location.marker,
                expectedPeer: identity.peerId,
                store: keyStore(for: location).requiringExistingKey())
            guard let revoked = try directory.revokedDevice() else { return (false, nil) }
            return (true, try revoked.complete())
        } catch {
            return (false, nil)
        }
    }

    /// How `searcher` names `peers`; waits like a session, as it opens the same folder's database.
    static func contactNames(
        searcher: ProfileSearcher, peers: [String]
    ) throws -> [ContactNameView] {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        return try searcher.contactNamesSlot(peerIds: peers)
    }

    /// Whether full peer photos load without a tap now; waits like a session for the same reason.
    static func peerPhotosLoadByThemselves(_ identity: ChatIdentity) throws -> Bool {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        return try withIdentity(identity.candidate, peerId: identity.peerId) { dir, key in
            try peerPhotoAuto(dir: dir, key: key)
        }
    }
}
