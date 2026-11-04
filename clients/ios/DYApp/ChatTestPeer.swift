#if DEBUG
import Foundation

/// A second identity on the local node for the two-peer UI tests. XCUITest runs out of process and
/// cannot reach the FFI, so the app under test hosts the peer behind a launch argument.
enum ChatTestPeer {
    static let greeting = "Hello from the peer"
    static let reply = "Hello back"
    static let confirmation = "Got your reply"

    static var enabled: Bool { ProcessInfo.processInfo.arguments.contains("--ui-test-chat-peer") }

    /// Sends the first message once `me` has published, waits for the reply and confirms it.
    static func start(me: String) {
        Thread.detachNewThread {
            do {
                try converse(with: me, session: published())
            } catch {
                NSLog("ChatTestPeer stopped: %@", String(describing: error))
            }
        }
    }

    private static func published() throws -> ChatSession {
        setTestNodeIdPowBits(bits: 8)
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("peer-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let key = IdentityKey.generate()
        let draft = ProfileDraft(values: ["gender": 1, "birthYear": 1990, "birthMonth": 1, "birthDay": 1])
        try saveProfileDraft(dir: dir.path, key: key, draft: ProfileRustDraftStore.record(from: draft))
        let checker = try ConnectionCheck(dir: dir.path)
        // The local node runs the public network: an empty name.
        retrying {
            _ = try checker.run(draft: NetworkSettings(seeds: ["127.0.0.1:33445"], name: ""), waitMs: 15_000)
            _ = try checker.activate()
        }
        let solver = try ProfileSolve(dir: dir.path, key: key)
        while try !solver.runStep(attested: true).solved {}
        // shortcut: the peer's Keychain wrapping key stays behind after a run, harmless on a test machine.
        let store = PlatformKeyStore.forSlot(UUID())
        let session = try ChatSession.open(
            dir: dir.path, key: key, deviceKey: IdentityKey.generate(), store: store,
            wrappedStateKey: newChatStateKey(store: store)
        )
        retrying { _ = try session.publishProfile(attested: true) }
        return session
    }

    private static func converse(with me: String, session: ChatSession) {
        // A failed send may have stored the text, and a second first message would be refused.
        retrying {
            if try session.chatHistory(peerId: me).messages.isEmpty {
                _ = try session.sendText(peerId: me, text: greeting)
            }
        }
        retrying {
            _ = try session.receive()
            let messages = try session.chatHistory(peerId: me).messages
            guard messages.contains(where: { $0.author == me && $0.text == reply }) else {
                throw CocoaError(.fileReadNoSuchFile)
            }
        }
        retrying { _ = try session.sendText(peerId: me, text: confirmation) }
    }

    /// Runs until it works: the test's own wait ends a peer that never gets there.
    private static func retrying(_ action: () throws -> Void) {
        while true {
            do {
                return try action()
            } catch {
                NSLog("ChatTestPeer retries: %@", String(describing: error))
                Thread.sleep(forTimeInterval: 2)
            }
        }
    }
}
#endif
