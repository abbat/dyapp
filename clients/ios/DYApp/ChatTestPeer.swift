#if DEBUG
    import Foundation

    /// A second identity on the local node for the two-peer UI tests. XCUITest runs out of process and
    /// cannot reach the FFI, so the app under test hosts the peer behind a launch argument.
    enum ChatTestPeer {
        static let greeting = "Hello from the peer"
        static let reply = "Hello back"
        static let confirmation = "Got your reply"
        static let sharedProfileReceived = "Shared profile received"

        static var enabled: Bool {
            ProcessInfo.processInfo.arguments.contains("--ui-test-chat-peer") || likesFirst
                || viewsFirst
                || sharesProfile || forwardsToPeer
        }

        private static var forwardsToPeer: Bool {
            ProcessInfo.processInfo.arguments.contains("--ui-test-chat-forward")
        }
        private static var likesFirst: Bool {
            ProcessInfo.processInfo.arguments.contains("--ui-test-chat-peer-likes")
        }
        private static var viewsFirst: Bool {
            ProcessInfo.processInfo.arguments.contains("--ui-test-chat-peer-views")
        }

        private static var sharesProfile: Bool {
            ProcessInfo.processInfo.arguments.contains("--ui-test-chat-peer-shares")
        }

        /// Sends the first message once `me` has published, waits for the reply and confirms it. With
        /// the likes argument it likes `me` and stops instead; with the shares argument its confirmation
        /// is an ordinary link naming the app owner, followed by an acknowledgement after receipt.
        static func start(me: String) {
            Thread.detachNewThread {
                do {
                    let (session, _) = try published()
                    if viewsFirst {
                        try sendView(to: me, session: session)
                    } else if likesFirst {
                        try retrying("like") { _ = try session.like(peerId: me, liked: true) }
                    } else {
                        let answer = try replyContent(to: me)
                        try converse(with: me, session: session, answer: answer)
                        if forwardsToPeer {
                            startForwardDestination(me: me)
                            try sendForwardSourcePhoto(to: me, session: session)
                        }
                    }
                } catch {
                    NSLog("ChatTestPeer stopped: %@", String(describing: error))
                }
            }
        }

        private static func sendForwardSourcePhoto(to peer: String, session: ChatSession) throws {
            let photo = try PhotoPrep.prepare(
                PhotoPrep.samplePNG(width: 640, height: 480, red: 0.5, alpha: 1))
            try retrying("forward source photo commitment") {
                _ = try session.submitChatPhoto(
                    peerId: peer, photo: photo.full, preview: photo.thumbnail,
                    width: photo.width, height: photo.height)
            }
        }

        private static func startForwardDestination(me: String) {
            Thread.detachNewThread {
                do {
                    let (session, _) = try published()
                    try retrying("forward destination greeting") {
                        _ = try session.submitText(peerId: me, text: "Forward destination")
                    }
                    try retrying("forward destination receipt") {
                        _ = try session.receive()
                        let messages = try session.chatHistory(peerId: me).messages
                        guard
                            messages.contains(where: {
                                $0.author == me && $0.forwarded && $0.text == greeting
                                    && !$0.deleted
                            })
                        else {
                            throw CocoaError(.fileReadNoSuchFile)
                        }
                    }
                    try retrying("forward destination acknowledgement") {
                        _ = try session.submitText(peerId: me, text: "Forwarded text received")
                    }
                    try retrying("forward destination photo receipt") {
                        _ = try session.receive()
                        let photos = try session.chatPhotos(peerId: me)
                        guard photos.contains(where: { $0.author == me && $0.forwarded }) else {
                            throw CocoaError(.fileReadNoSuchFile)
                        }
                    }
                    try retrying("forward destination photo acknowledgement") {
                        _ = try session.submitText(peerId: me, text: "Forwarded photo received")
                    }
                } catch {
                    NSLog("ChatTestPeer forward destination stopped: %@", String(describing: error))
                }
            }
        }

        private static func sendView(to peer: String, session: ChatSession) throws {
            try session.setProfileViewsEnabled(enabled: true)
            try retrying("view recipient publication") {
                guard try !session.profiles(peerIds: [peer]).isEmpty else {
                    throw CocoaError(.fileReadNoSuchFile)
                }
            }
            let opening = try session.profileViewOpening(peerId: peer)
            guard let submission = try session.submitProfileView(opening: opening) else {
                throw CocoaError(.fileWriteUnknown)
            }
            if case .queued = submission {
                try retrying("view outbox") { _ = try session.receive() }
            }
        }

        private static func replyContent(to peer: String) throws -> String {
            guard sharesProfile else { return confirmation }
            guard let link = profileLinkText(peerId: peer) else {
                throw CocoaError(.fileReadCorruptFile)
            }
            return link
        }

        private static func published() throws -> (ChatSession, peer: String) {
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent(
                "peer-\(UUID().uuidString)")
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let key = try IdentityKey.generate()
            let draft = ProfileDraft(values: [
                "gender": 1, "birthYear": 1_990, "birthMonth": 1, "birthDay": 1,
            ])
            try saveProfileDraft(
                dir: dir.path, key: key, draft: ProfileRustDraftStore.record(from: draft))
            let checker = try ConnectionCheck(dir: dir.path)
            // The local node runs the public network: an empty name.
            try retrying("network activation") {
                _ = try checker.run(
                    draft: NetworkSettings(seeds: ["127.0.0.1:33445"], name: ""), waitMs: 15_000)
                _ = try checker.activate()
            }
            let solver = try ProfileSolve(dir: dir.path, key: key)
            while try !solver.runStep(attested: true).solved {}
            if likesFirst {
                let photo = try PhotoPrep.prepare(
                    PhotoPrep.samplePNG(width: 640, height: 480, red: 0.5, alpha: 1))
                _ = try addProfilePhoto(
                    dir: dir.path, key: key, full: photo.full, thumbnail: photo.thumbnail)
            }
            // shortcut: the peer's Keychain wrapping key stays behind after a run, harmless on a test machine.
            let store = PlatformKeyStore.forSlot(UUID())
            let session = try ChatSession.open(
                dir: dir.path, key: key, deviceKey: try IdentityKey.generate(), store: store,
                wrappedStateKey: newChatStateKey(store: store)
            )
            try retrying("profile publication") { _ = try session.publishProfile(attested: true) }
            return (session, key.peerId())
        }

        private static func converse(with me: String, session: ChatSession, answer: String) throws {
            // Accepted and Queued both committed; retry only a precommit failure.
            try retrying("greeting commitment") {
                _ = try session.submitText(peerId: me, text: greeting)
            }
            try retrying("owner reply") {
                _ = try session.receive()
                let messages = try session.chatHistory(peerId: me).messages
                guard messages.contains(where: { $0.author == me && $0.text == reply }) else {
                    throw CocoaError(.fileReadNoSuchFile)
                }
            }
            if forwardsToPeer { return }
            try retrying("answer commitment") {
                _ = try session.submitText(peerId: me, text: answer)
            }
            if sharesProfile {
                try retrying("shared owner profile receipt") {
                    _ = try session.receive()
                    let received = try session.chatHistory(peerId: me).messages.filter {
                        $0.author == me && $0.text == answer && !$0.deleted
                    }
                    guard received.count == 1 else { throw CocoaError(.fileReadNoSuchFile) }
                }
                try retrying("profile receipt acknowledgement") {
                    _ = try session.submitText(peerId: me, text: sharedProfileReceived)
                }
            }
        }

        /// Native IO completion drives the next check; every protocol transition gets a new deadline.
        private static func retrying(_ stage: String, _ action: () throws -> Void) throws {
            let deadline = ProcessInfo.processInfo.systemUptime + 150
            var failure: Error = CocoaError(.fileReadNoSuchFile)
            repeat {
                do {
                    return try action()
                } catch {
                    failure = error
                }
            } while ProcessInfo.processInfo.systemUptime < deadline
            NSLog("ChatTestPeer deadline for %@: %@", stage, String(describing: failure))
            throw failure
        }
    }
#endif
