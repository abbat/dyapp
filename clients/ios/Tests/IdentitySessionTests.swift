@testable import DYApp
import XCTest

private struct IdentityTestFailure: Error {}

/// Tests choose the completion order rather than waiting for a scheduler to happen to race.
private final class HeldIdentityIO: @unchecked Sendable {
    private struct Entry: Sendable {
        let work: IdentityIO.Work
        let completion: IdentityIO.Completion
    }

    private let lock = NSLock()
    private var entries: [Entry] = []
    private let entered = (0..<4).map { XCTestExpectation(description: "Identity IO entry \($0)") }

    func run(_ work: @escaping IdentityIO.Work, _ completion: @escaping IdentityIO.Completion) {
        lock.lock()
        let index = entries.count
        entries.append(Entry(work: work, completion: completion))
        lock.unlock()
        entered[index].fulfill()
    }

    func waitForEntry(_ index: Int) async -> Bool {
        await withCheckedContinuation { continuation in
            DispatchQueue.global().async { [self] in
                continuation.resume(
                    returning: XCTWaiter.wait(for: [entered[index]], timeout: 10) == .completed)
            }
        }
    }

    @MainActor func complete(_ index: Int) {
        let entry = entry(at: index)
        entry.completion(entry.work())
    }

    private func entry(at index: Int) -> Entry {
        lock.lock()
        defer { lock.unlock() }
        return entries[index]
    }
}

@MainActor
final class IdentitySessionTests: XCTestCase {
    func testConstructionDoesNotReadAndLoadingCannotOfferCreationBeforeConfirmedAbsence() {
        let queue = HeldIdentityIO()
        let model = IdentitySession(read: { .setup }, run: queue.run)
        XCTAssertNil(model.resolution)
        model.reload()
        XCTAssertNil(model.resolution)
        queue.complete(0)
        XCTAssertEqual(model.resolution, .setup)
    }

    func testRepeatedScreenStartDoesNotDuplicateAReadOrReplaceTheLoadedSelection() {
        let queue = HeldIdentityIO()
        let selected = ready()
        let model = IdentitySession(read: { selected }, run: queue.run)
        model.start()
        model.start()
        queue.complete(0)
        XCTAssertEqual(model.resolution, selected)
        model.start()
        XCTAssertEqual(model.resolution, selected)
    }

    func testFailedReadStaysInRecoveryAndRetryCannotAcceptAnObsoleteRead() {
        let queue = HeldIdentityIO()
        let model = IdentitySession(read: { throw IdentityTestFailure() }, run: queue.run)
        model.reload()
        model.reload()
        queue.complete(0)
        XCTAssertNil(model.resolution)
        queue.complete(1)
        XCTAssertEqual(model.resolution, .recovery(nil, .unavailable))
    }

    func testMutationSurvivesCancelledInitiatorAndAnOlderReadCannotOverwriteIt() async {
        let queue = HeldIdentityIO()
        let model = IdentitySession(read: { .setup }, run: queue.run)
        model.reload()
        let selected = ready()
        let action = Task {
            await model.perform {
                IdentityUpdate(resolution: selected, durabilityDetail: "not yet synced")
            }
        }
        let entered = await queue.waitForEntry(1)
        XCTAssertTrue(entered)
        XCTAssertTrue(model.busy)
        let rejected = await model.perform {
            XCTFail("A second mutation must not start"); return IdentityUpdate(resolution: .setup)
        }
        XCTAssertFalse(rejected)
        action.cancel()
        queue.complete(1)
        let accepted = await action.value
        XCTAssertTrue(accepted)
        XCTAssertEqual(model.resolution, selected)
        XCTAssertEqual(model.persistenceDetail, "not yet synced")
        queue.complete(0)
        XCTAssertEqual(model.resolution, selected)
        XCTAssertFalse(model.busy)
        model.reload()
        queue.complete(2)
        XCTAssertEqual(model.persistenceDetail, "not yet synced")
        model.acknowledgePersistence()
        XCTAssertNil(model.persistenceDetail)
    }

    func testLinkReceiptKeepsDurabilityNoticeAfterRegistryReadback() {
        let queue = HeldIdentityIO()
        let selected = ready()
        let model = IdentitySession(read: { selected }, run: queue.run)

        model.reloadAfterLink("not yet synced")
        XCTAssertNil(model.resolution)
        XCTAssertEqual(model.persistenceDetail, "not yet synced")
        queue.complete(0)
        XCTAssertEqual(model.resolution, selected)
        XCTAssertEqual(model.persistenceDetail, "not yet synced")
    }

    func testFailedMutationReadsAuthoritativeStateWithoutRetryingTheWrite() async {
        let queue = HeldIdentityIO()
        let selected = ready()
        let model = IdentitySession(read: { selected }, run: queue.run)
        let action = Task { await model.perform { throw IdentityTestFailure() } }
        let entered = await queue.waitForEntry(0)
        XCTAssertTrue(entered)
        queue.complete(0)
        let succeeded = await action.value
        XCTAssertFalse(succeeded)
        XCTAssertEqual(model.resolution, selected)
        XCTAssertFalse(model.busy)
    }

    private func ready() -> IdentityResolution {
        .ready(IdentityCandidate(location: .slot(UUID()), state: .ready), "peer")
    }
}
