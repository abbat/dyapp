import Foundation

/// Stable, Sendable outcomes used by both Apple screens instead of raw UniFFI enum values.
enum NetworkRowOutcome: Equatable, Sendable {
    case connected(String)
    case invalidAddress
    case invalidPeerID
    case duplicate
    case unreachable
    case wrongPeer
    case wrongNetwork

    init(_ row: SeedRow) {
        switch row {
        case let .connected(peerID):
            self = .connected(peerID)

        case .invalidAddress:
            self = .invalidAddress

        case .invalidPeerId:
            self = .invalidPeerID

        case .duplicate:
            self = .duplicate

        case .unreachable:
            self = .unreachable

        case .wrongPeer:
            self = .wrongPeer

        case .wrongNetwork:
            self = .wrongNetwork
        }
    }
}

enum NetworkCheckAttempt: Sendable {
    case completed(NetworkConnectionSession, [NetworkRowOutcome])
    case failed
}

/// The Rust object is synchronized internally so its check and cancellation can use separate threads.
final class NetworkConnectionSession: @unchecked Sendable {
    private let checker: ConnectionCheck
    private let attempt: ConnectionAttempt

    var isCancelled: Bool { attempt.isCancelled() }

    init(checker: ConnectionCheck) {
        self.checker = checker
        attempt = ConnectionAttempt(checker: checker)
    }

    /// Explicit unbound directories are retained for native loopback fixtures only.
    convenience init(directory: URL) throws {
        try self.init(checker: ConnectionCheck(dir: directory.path))
    }

    convenience init(candidate: IdentityCandidate, peerId: String) throws {
        try self.init(
            checker: ConnectionCheck.forSlot(
                registry: RustBridge.identityStore().registry,
                id: candidate.location.marker, expectedPeer: peerId))
    }

    /// Construct off the UI actor, then retain this session so Cancel can reach a running check.
    static func create(candidate: IdentityCandidate, peerId: String) async
        -> NetworkConnectionSession?
    {
        await IdentityIO.performBlocking {
            try? NetworkConnectionSession(candidate: candidate, peerId: peerId)
        }
    }

    static func persist(candidate: IdentityCandidate, peerId: String, rows: [String], name: String)
        async -> Bool
    {
        await IdentityIO.performBlocking { () -> Bool in
            do {
                try RustBridge.storeNetworkDraft(
                    for: candidate, peerId: peerId, draft: NetworkSettings(seeds: rows, name: name)
                )
                return true
            } catch {
                return false
            }
        }
    }

    func runAsync(rows: [String], name: String) async -> NetworkCheckAttempt {
        await IdentityIO.performBlocking { () -> NetworkCheckAttempt in
            do {
                return .completed(self, try self.run(rows: rows, name: name))
            } catch {
                return .failed
            }
        }
    }

    func cancel() { attempt.cancel() }

    /// Only the Rust checker that reached a compatible node can write active settings.
    func activate() throws {
        guard !isCancelled else { throw CheckError.Cancelled }
        defer { MailJob.wakeSearch(afterQueueChange: true) }
        _ = try checker.activate()
    }

    func activateAsync() async -> Bool {
        await IdentityIO.performBlocking {
            do {
                try self.activate()
                return true
            } catch {
                return false
            }
        }
    }

    /// Single-use blocking UniFFI call; dispatch through native IO, never the UI actor.
    func run(rows: [String], name: String, waitMs: UInt32 = 5_000) throws -> [NetworkRowOutcome] {
        let results = try attempt.run(
            draft: NetworkSettings(seeds: rows, name: name), waitMs: waitMs)
        return results.map(NetworkRowOutcome.init)
    }
}
