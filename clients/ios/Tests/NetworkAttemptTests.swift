@testable import DYApp
import XCTest

private struct NetworkAttemptFixture {
    let root: URL
    let keys: IdentityAdmissionKeys
    let store: IdentitySlotStore
    let id: UUID
    let peer: String
    let checker: ConnectionCheck

    var draftPath: String { directory.appendingPathComponent("network-draft").path }

    var directory: URL {
        root.appendingPathComponent("profiles").appendingPathComponent(id.uuidString.lowercased())
    }

    init() throws {
        root = try identitySlotTestRoot()
        keys = IdentityAdmissionKeys()
        store = IdentitySlotStore(root: root, keys: keys)
        let created = try store.create()
        id = created.slot.id
        peer = created.peerId
        checker = try ConnectionCheck.forSlot(
            registry: store.registry, id: id.uuidString.lowercased(), expectedPeer: peer)
    }
}

final class NetworkAttemptTests: XCTestCase {
    func testCancelDuringRealAdmissionStopsBeforeDraftWriteAndRetryHasItsOwnOwner() async throws {
        let fixture = try NetworkAttemptFixture()
        let session = NetworkConnectionSession(checker: fixture.checker)
        let keys = fixture.keys
        keys.arm()
        defer { keys.resume() }
        let worker = Task.detached { () -> Result<[NetworkRowOutcome], Error> in
            defer { keys.finished.signal() }
            return Result { try session.run(rows: ["not an address"], name: "", waitMs: 0) }
        }
        let entered = await keys.entered.wait()
        guard entered else { return XCTFail("The worker never reached actual admission") }
        session.cancel()
        XCTAssertTrue(session.isCancelled)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.draftPath))
        keys.resume()
        let completed = await keys.finished.wait()
        guard completed else { return XCTFail("The cancelled admitted worker did not complete") }
        let outcome = await worker.value
        guard case .failure(CheckError.Cancelled) = outcome else {
            return XCTFail("The owned attempt must report cancellation: \(outcome)")
        }
        XCTAssertThrowsError(try session.activate())
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.draftPath))
        let retry = NetworkConnectionSession(checker: fixture.checker)
        XCTAssertFalse(retry.isCancelled)
        XCTAssertEqual(
            try retry.run(rows: ["not an address"], name: "", waitMs: 0), [.invalidAddress])
        XCTAssertTrue(session.isCancelled)
        XCTAssertEqual(
            try XCTUnwrap(loadNetwork(dir: fixture.directory.path, file: .draft)).seeds,
            ["not an address"])
    }

    func testIdleCheckerDoesNotPreventRemovalAndCannotRecreateRemovedStorage() async throws {
        let fixture = try NetworkAttemptFixture()
        let session = NetworkConnectionSession(checker: fixture.checker)
        let store = fixture.store
        let (id, peer) = (fixture.id, fixture.peer)
        let finished = NativeTestSignal("Removal with an idle bound checker")
        let removal = Task.detached {
            defer { finished.signal() }
            return Result { try store.remove(id, confirming: peer) }
        }
        let completed = await finished.wait()
        guard completed else { return XCTFail("An idle worker retained native admission") }
        let result = await removal.value
        _ = try result.get()
        XCTAssertThrowsError(try session.run(rows: ["not an address"], name: "", waitMs: 0))
        XCTAssertThrowsError(try session.activate())
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.directory.path))
    }
}
