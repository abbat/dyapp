import Combine
import Foundation

struct IdentityChange: Sendable {
    let update: IdentityUpdate
    let succeeded: Bool
}

/// Native registry locks and Keychain calls must not occupy the UI or cooperative executor.
enum IdentityIO {
    typealias Work = @Sendable () -> IdentityChange
    typealias Completion = @MainActor @Sendable (IdentityChange) -> Void
    typealias Runner = @Sendable (@escaping Work, @escaping Completion) -> Void

    private static let queue = DispatchQueue(label: "com.dyapp.identity", qos: .userInitiated)
    private static let workers = DispatchQueue(
        label: "com.dyapp.identity.workers", qos: .userInitiated, attributes: .concurrent)

    static func performBlocking<Value: Sendable>(_ work: @escaping @Sendable () -> Value) async
        -> Value
    {
        await withCheckedContinuation { continuation in
            workers.async { continuation.resume(returning: work()) }
        }
    }

    // Editor reads follow previously admitted writes, including across view recreation.
    private static let profileQueue = DispatchQueue(label: "com.dyapp.profile", qos: .userInitiated)

    @MainActor
    static func performProfile<Value: Sendable>(_ work: @escaping @Sendable () -> Value) async
        -> Value
    {
        await withCheckedContinuation { continuation in
            profileQueue.async { continuation.resume(returning: work()) }
        }
    }

    static func perform(_ work: @escaping Work, completion: @escaping Completion) {
        queue.async {
            let change = work()
            Task { @MainActor in completion(change) }
        }
    }
}

/// App-owned presentation state; a disappearing initiator cannot discard a committed selection.
@MainActor
final class IdentitySession: ObservableObject {
    @Published private(set) var resolution: IdentityResolution?
    @Published private(set) var busy = false
    @Published private(set) var persistenceDetail: String?
    private let read: @Sendable () throws -> IdentityResolution
    private let run: IdentityIO.Runner
    private var readTicket: UUID?

    init(
        read: @escaping @Sendable () throws -> IdentityResolution = {
            try RustBridge.identityState()
        },
        run: @escaping IdentityIO.Runner = { IdentityIO.perform($0, completion: $1) }
    ) {
        self.read = read
        self.run = run
    }

    func start() {
        guard resolution == nil, readTicket == nil else { return }
        reload()
    }

    /// Called only after unlock; construction never reads protected identity storage.
    func reload() {
        guard !busy else { return }
        let ticket = UUID()
        readTicket = ticket
        resolution = nil
        let read = read
        run(
            {
                IdentityChange(
                    update: IdentityUpdate(
                        resolution: (try? read()) ?? .recovery(nil, .unavailable)),
                    succeeded: true)
            },
            { [weak self] change in
                guard let self, self.readTicket == ticket else { return }
                self.readTicket = nil
                self.resolution = change.update.resolution
            })
    }

    /// Admission and delivery belong to the app, independently of the initiating SwiftUI task.
    func perform(_ work: @escaping @Sendable () throws -> IdentityUpdate) async -> Bool {
        guard !busy, !Task.isCancelled else { return false }
        busy = true
        readTicket = nil
        let read = read
        return await withCheckedContinuation { continuation in
            run(
                {
                    do {
                        return IdentityChange(update: try work(), succeeded: true)
                    } catch {
                        return IdentityChange(
                            update: IdentityUpdate(
                                resolution: (try? read()) ?? .recovery(nil, .unavailable)),
                            succeeded: false)
                    }
                },
                { [self] change in
                    resolution = change.update.resolution
                    // Readback and a later durable commit cannot silently acknowledge an earlier warning.
                    if let detail = change.update.durabilityDetail { persistenceDetail = detail }
                    busy = false
                    continuation.resume(returning: change.succeeded)
                })
        }
    }

    func reloadAfterSlotChange(_ detail: String?) {
        if let detail { persistenceDetail = detail }
        reload()
    }

    func acknowledgePersistence() { persistenceDetail = nil }
}
