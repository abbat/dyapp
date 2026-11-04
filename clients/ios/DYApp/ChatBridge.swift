import Foundation

/// The identity whose chats a screen shows.
struct ChatIdentity: Sendable {
    let candidate: IdentityCandidate
    let peerId: String
}

/// The file's wrapped key, made once: a replaced key could no longer open the saved MLS state.
func wrappedKeyFile(_ file: URL, make: () throws -> Data) throws -> Data {
    if !FileManager.default.fileExists(atPath: file.path) {
        try make().write(to: file, options: .atomic)
    }
    return try Data(contentsOf: file)
}

extension RustBridge {
    /// One session at a time: two would advance the same saved MLS state and the later one would
    /// drop the other's commit. Removing an identity waits too, so a receive cannot write into its folder.
    static let chatsLock = NSLock()

    /// Runs a blocking `action` on this identity's chats; call it off the main thread.
    static func withChats<T>(_ identity: ChatIdentity, _ action: (ChatSession) throws -> T) throws -> T {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        let location = identity.candidate.location
        let key = try identityStore().open(location)
        guard key.peerId() == identity.peerId else { throw IdentitySlotError.peerIDMismatch }
        let dir = try networkDirectory(root: directory(), candidate: identity.candidate)
        let store = keyStore(for: location)
        let device = try wrappedKeyFile(dir.appendingPathComponent("device.key")) {
            try IdentityKey.generate().wrap(store: store)
        }
        let state = try wrappedKeyFile(dir.appendingPathComponent("chat-state.key")) {
            try newChatStateKey(store: store)
        }
        let session = try ChatSession.open(
            dir: dir.path, key: key, deviceKey: IdentityKey.unwrap(store: store, wrapped: device),
            store: store, wrappedStateKey: state
        )
        return try action(session)
    }

    /// How `searcher` names `peers`; waits like a session, as it opens the same folder's database.
    static func contactNames(
        _ identity: ChatIdentity, searcher: ProfileSearcher, peers: [String]
    ) throws -> [ContactNameView] {
        chatsLock.lock()
        defer { chatsLock.unlock() }
        let key = try identityStore().open(identity.candidate.location)
        guard key.peerId() == identity.peerId else { throw IdentitySlotError.peerIDMismatch }
        return try searcher.contactNames(key: key, peerIds: peers)
    }
}
