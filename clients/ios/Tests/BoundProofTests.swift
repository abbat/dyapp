@testable import DYApp
import XCTest

final class BoundProofTests: XCTestCase {
    func testAnIdleProofCannotOpenStorageAfterItsSlotBecomesIncomplete() async throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let id = created.slot.id.uuidString.lowercased()
        let directory = root.appendingPathComponent("profiles").appendingPathComponent(id)
        let wrapped = try Data(contentsOf: directory.appendingPathComponent("identity.key"))
        let session = ProfileSolveSession(
            solver: try ProfileSolve.forSlot(
                registry: store.registry, id: id, expectedPeer: created.peerId))
        try Data("pending\n".utf8).write(
            to: directory.appendingPathComponent("state"), options: .atomic)
        guard case .failed? = await step(session) else {
            return XCTFail("An incomplete identity must refuse proof admission")
        }
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: directory.appendingPathComponent("client.db").path))
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("identity.key")), wrapped)
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("state")), Data("pending\n".utf8))
    }

    private func step(_ session: ProfileSolveSession) async -> ProfileWorkAttempt? {
        let finished = NativeTestSignal("Actual proof step completion")
        let work = Task {
            let result = await session.runStep(attested: true)
            finished.signal()
            return result
        }
        let completed = await finished.wait()
        guard completed else { XCTFail("The proof step did not complete"); return nil }
        return await work.value
    }

    func testIdleProofDoesNotPreventRemovalAndItsLaterStepCannotRecreateStorage() async throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let id = created.slot.id
        let peer = created.peerId
        let solver = try ProfileSolve.forSlot(
            registry: store.registry, id: id.uuidString.lowercased(), expectedPeer: peer)
        let session = ProfileSolveSession(solver: solver)
        let finished = NativeTestSignal("Removal with an idle proof worker")
        let removal = Task.detached {
            defer { finished.signal() }
            return Result { try store.remove(id, confirming: peer) }
        }
        let completed = await finished.wait()
        guard completed else { return XCTFail("An idle proof worker retained native admission") }
        let result = await removal.value
        _ = try result.get()
        guard case .failed? = await step(session) else {
            return XCTFail("A removed identity must refuse proof work")
        }
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: root.appendingPathComponent("profiles")
                    .appendingPathComponent(id.uuidString.lowercased()).path))
    }
}
