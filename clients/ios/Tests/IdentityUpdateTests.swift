@testable import DYApp
import XCTest

final class IdentityUpdateTests: XCTestCase {
    func testVisibleCommitRetainsReadyIdentityAndItsIndependentPersistenceWarning() throws {
        let id = UUID()
        let key = try IdentityKey.generate()
        let slot = MobileSlot(id: id.uuidString.lowercased(), directory: "unused", state: .ready)
        for detail: String? in [nil, "directory synchronization unavailable"] {
            let update = try IdentityUpdate(
                native: MobileSlotSelection(slot: slot, key: key, durabilityDetail: detail))
            XCTAssertEqual(
                update.resolution,
                .ready(IdentityCandidate(location: .slot(id), state: .ready), key.peerId()))
            XCTAssertEqual(
                update.durabilityDetail, detail,
                "A visible commit must not become a retryable failure")
        }
    }

    func testIncompleteNativeResolutionRetainsTheKnownPeerWithoutOpeningIt() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let created = try store.create()
        let marker = root.appendingPathComponent("profiles")
            .appendingPathComponent(created.slot.id.uuidString.lowercased()).appendingPathComponent(
                "state")
        try Data("pending\n".utf8).write(to: marker, options: .atomic)
        XCTAssertEqual(
            try store.resolve(),
            .recovery(.slot(created.slot.id), .incomplete, peerId: created.peerId))
        XCTAssertThrowsError(try store.open(created.slot))
        XCTAssertEqual(try Data(contentsOf: marker), Data("pending\n".utf8))
    }
}
