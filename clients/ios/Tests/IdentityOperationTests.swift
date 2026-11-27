import Darwin
@testable import DYApp
import XCTest

final class IdentityOperationTests: XCTestCase {
    func testSwiftScopeRetainsNativeMutationExclusionUntilItsActionReturns() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let lockPath = root.appendingPathComponent(".identity-slots.lock").path
        let handle = open(lockPath, O_RDWR)
        guard handle >= 0 else { return XCTFail("The actual registry lock must exist") }
        defer { XCTAssertEqual(close(handle), 0) }
        let peer = try store.withIdentity(.slot(created.slot.id), peerId: created.peerId) {
            dir, key in
            XCTAssertEqual(
                dir,
                root.appendingPathComponent("profiles")
                    .appendingPathComponent(created.slot.id.uuidString.lowercased()).path)
            let attempt = flock(handle, LOCK_EX | LOCK_NB)
            let error = errno
            XCTAssertEqual(
                attempt, -1, "The actual native operation must still own its shared lock")
            XCTAssertEqual(error, EWOULDBLOCK)
            XCTAssertEqual(key.peerId(), created.peerId)
            return key.peerId()
        }
        XCTAssertEqual(peer, created.peerId)
        XCTAssertEqual(flock(handle, LOCK_EX | LOCK_NB), 0, "Return must release the operation")
        XCTAssertEqual(flock(handle, LOCK_UN), 0)
    }

    func testWrongPeerAndIncompleteSlotRefuseBeforeTheActionWithoutReplacingKeys() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let directory = root.appendingPathComponent("profiles")
            .appendingPathComponent(created.slot.id.uuidString.lowercased())
        let wrapped = try Data(contentsOf: directory.appendingPathComponent("identity.key"))
        XCTAssertThrowsError(
            try store.withIdentity(.slot(created.slot.id), peerId: "wrong peer") { _, _ in
                XCTFail("A mismatched operation must not run its action")
            })
        try Data("pending\n".utf8).write(
            to: directory.appendingPathComponent("state"), options: .atomic)
        XCTAssertThrowsError(
            try store.withIdentity(.slot(created.slot.id), peerId: created.peerId) { _, _ in
                XCTFail("An incomplete identity must not run its action")
            })
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("identity.key")), wrapped)
        XCTAssertEqual(
            try Data(contentsOf: directory.appendingPathComponent("state")), Data("pending\n".utf8))
    }
}
