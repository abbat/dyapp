@testable import DYApp
import XCTest

final class BoundSearchTests: XCTestCase {
    func testBoundNameReadUsesTheRealExistingIdentityAndPreservesItsKey() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let searcher = try ProfileSearcher.forSlot(
            registry: store.registry, id: created.slot.id.uuidString.lowercased(),
            expectedPeer: created.peerId)
        let views = try searcher.contactNamesSlot(peerIds: [created.peerId])
        XCTAssertEqual(views.map(\.peerId), [created.peerId])
        XCTAssertEqual(try store.open(created.slot).peerId(), created.peerId)
    }

    func testIncompleteSlotRefusesSearchAndNamesBeforeOpeningDatabase() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let id = created.slot.id.uuidString.lowercased()
        let directory = root.appendingPathComponent("profiles").appendingPathComponent(id)
        let wrapped = try Data(contentsOf: directory.appendingPathComponent("identity.key"))
        let searcher = try ProfileSearcher.forSlot(
            registry: store.registry, id: id, expectedPeer: created.peerId)
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: directory.appendingPathComponent("client.db").path))
        try Data("pending\n".utf8).write(
            to: directory.appendingPathComponent("state"), options: .atomic)
        XCTAssertThrowsError(
            try searcher.searchSlot(
                conditions: [], employer: searchEmployerFilter(mode: .any, text: ""), limit: 0))
        XCTAssertThrowsError(try searcher.contactNamesSlot(peerIds: [created.peerId]))
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: directory.appendingPathComponent("client.db").path))
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("identity.key")), wrapped)
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("state")), Data("pending\n".utf8))
    }

    func testIdleSearcherAllowsRemovalAndCannotRecreateRemovedStorage() async throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let (id, peer) = (created.slot.id, created.peerId)
        let searcher = try ProfileSearcher.forSlot(
            registry: store.registry, id: id.uuidString.lowercased(), expectedPeer: peer)
        let finished = NativeTestSignal("Removal with an idle search worker")
        let removal = Task.detached {
            defer { finished.signal() }
            return Result { try store.remove(id, confirming: peer) }
        }
        let completed = await finished.wait()
        guard completed else { return XCTFail("An idle searcher retained native admission") }
        let result = await removal.value
        _ = try result.get()
        XCTAssertThrowsError(try searcher.contactNamesSlot(peerIds: [peer]))
        XCTAssertThrowsError(
            try searcher.searchSlot(
                conditions: [], employer: searchEmployerFilter(mode: .any, text: ""), limit: 0))
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: root.appendingPathComponent("profiles").appendingPathComponent(
                    id.uuidString.lowercased()
                ).path))
    }
}
