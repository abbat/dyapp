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

    init(directory: URL) throws {
        checker = try ConnectionCheck(dir: directory.path)
    }

    convenience init(candidate: IdentityCandidate) throws {
        let directory = RustBridge.networkDirectory(root: try RustBridge.directory(), candidate: candidate)
        try self.init(directory: directory)
    }

    static func start(candidate: IdentityCandidate, rows: [String], name: String) async -> NetworkCheckAttempt {
        guard let session = await create(candidate: candidate) else { return .failed }
        return await session.runAsync(rows: rows, name: name)
    }

    /// Construct off the UI actor, then retain this session so Cancel can reach a running check.
    static func create(candidate: IdentityCandidate) async -> NetworkConnectionSession? {
        await Task.detached(priority: .userInitiated) {
            try? NetworkConnectionSession(candidate: candidate)
        }.value
    }

    static func persist(candidate: IdentityCandidate, rows: [String], name: String) async -> Bool {
        await Task.detached(priority: .userInitiated) { () -> Bool in
            do {
                try RustBridge.storeNetworkDraft(
                    for: candidate, draft: NetworkSettings(seeds: rows, name: name)
                )
                return true
            } catch {
                return false
            }
        }.value
    }

    func runAsync(rows: [String], name: String) async -> NetworkCheckAttempt {
        await Task.detached(priority: .userInitiated) { () -> NetworkCheckAttempt in
            do {
                return .completed(self, try self.run(rows: rows, name: name))
            } catch {
                return .failed
            }
        }.value
    }

    func cancel() {
        checker.cancel()
    }

    /// Only the Rust checker that reached a compatible node can write active settings.
    func activate() throws {
        _ = try checker.activate()
    }

    func activateAsync() async -> Bool {
        await Task.detached(priority: .userInitiated) {
            do {
                try self.activate()
                return true
            } catch {
                return false
            }
        }.value
    }

    /// Blocking UniFFI call; invoke from a detached task, never the UI actor.
    func run(rows: [String], name: String, waitMs: UInt32 = 5_000) throws -> [NetworkRowOutcome] {
        let results = try checker.run(draft: NetworkSettings(seeds: rows, name: name), waitMs: waitMs)
        return results.map(NetworkRowOutcome.init)
    }
}
