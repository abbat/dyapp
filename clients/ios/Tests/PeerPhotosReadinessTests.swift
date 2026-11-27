@testable import DYApp
import XCTest

private final class PhotoPreparationGate: @unchecked Sendable {
    let entered = NativeTestSignal("Photo worker preparation entry")
    private let roots: [String: URL]
    private let failFirst: Bool
    private let released = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var count = 0

    var calls: Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }

    init(roots: [String: URL], failFirst: Bool = false) {
        self.roots = roots
        self.failFirst = failFirst
    }

    func resume() { released.signal() }

    private func register() -> Int {
        lock.lock()
        defer { lock.unlock() }
        count += 1
        return count
    }

    func load(_ identity: ChatIdentity) throws -> ProfileSearcher {
        XCTAssertFalse(Thread.isMainThread, "Preparing native workers must not block UI")
        if register() == 1 {
            entered.signal()
            guard released.wait(timeout: .now() + 10) == .success else {
                throw CoreError.Failed(message: "The test did not release photo preparation")
            }
            if failFirst { throw CoreError.Failed(message: "Controlled preparation failure") }
        }
        let root = try XCTUnwrap(roots[identity.candidate.location.marker])
        return try ProfileSearcher(dir: root.path)
    }
}

@MainActor
final class PeerPhotosReadinessTests: XCTestCase {
    func testSamePeerInDifferentSlotsCannotReusePendingPreparationOrItsLateCompletion() async throws
    {
        let first = identity()
        let second = identity()
        let gate = PhotoPreparationGate(roots: [
            first.candidate.location.marker: try identitySlotTestRoot(),
            second.candidate.location.marker: try identitySlotTestRoot(),
        ])
        defer { gate.resume() }
        let photos = PeerPhotos(makeSearcher: gate.load)
        let oldDone = NativeTestSignal("Retired photo preparation completion")
        let old = Task {
            let worker = await photos.use(first)
            oldDone.signal()
            return worker
        }
        let entered = await gate.entered.wait()
        guard entered else { return XCTFail("The first preparation never entered") }
        let currentDone = NativeTestSignal("Successor photo preparation completion")
        let current = Task {
            let worker = await photos.use(second)
            currentDone.signal()
            return worker
        }
        let ready = await currentDone.wait()
        guard ready else { return XCTFail("The successor waited for obsolete preparation") }
        let prepared = await current.value
        let worker = try XCTUnwrap(prepared)
        let before = photos.fetched
        gate.resume()
        let retired = await oldDone.wait()
        guard retired else { return XCTFail("The old preparation did not complete") }
        let obsolete = await old.value
        XCTAssertNil(obsolete)
        XCTAssertEqual(
            photos.fetched, before, "An obsolete completion cannot refresh the successor")
        let cached = await photos.use(second)
        XCTAssertIdentical(cached?.searcher, worker.searcher)
        XCTAssertEqual(gate.calls, 2)
        photos.reset()
        XCTAssertEqual(photos.fetched, before + 1)
    }

    func testFailedPreparationAllowsAnIndependentRetryInTheSameSlot() async throws {
        let identity = identity()
        let gate = PhotoPreparationGate(
            roots: [identity.candidate.location.marker: try identitySlotTestRoot()], failFirst: true
        )
        defer { gate.resume() }
        let photos = PeerPhotos(makeSearcher: gate.load)
        let failedDone = NativeTestSignal("Failed photo preparation completion")
        let first = Task {
            let worker = await photos.use(identity)
            failedDone.signal()
            return worker
        }
        let entered = await gate.entered.wait()
        guard entered else { return XCTFail("The controlled factory did not enter") }
        gate.resume()
        let failed = await failedDone.wait()
        guard failed else { return XCTFail("The controlled factory failure did not complete") }
        let unavailable = await first.value
        XCTAssertNil(unavailable)
        let ready = NativeTestSignal("Fresh retry preparation completion")
        let retry = Task {
            let worker = await photos.use(identity)
            ready.signal()
            return worker
        }
        let prepared = await ready.wait()
        guard prepared else { return XCTFail("The fresh retry did not complete") }
        let available = await retry.value
        XCTAssertNotNil(available)
        XCTAssertEqual(gate.calls, 2)
    }

    private func identity() -> ChatIdentity {
        ChatIdentity(
            candidate: IdentityCandidate(location: .slot(UUID()), state: .ready),
            peerId: "same peer")
    }
}
