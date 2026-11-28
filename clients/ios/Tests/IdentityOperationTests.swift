import Darwin
@testable import DYApp
import XCTest

final class IdentityOperationTests: XCTestCase {
    func testCancelledPhotoWorkRetainsNativeAdmissionUntilCompletion() async throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let created = try store.create()
        let id = created.slot.id
        let peer = created.peerId
        let handle = open(root.appendingPathComponent(".identity-slots.lock").path, O_RDWR)
        guard handle >= 0 else { return XCTFail("The registry lock must exist") }
        defer { XCTAssertEqual(close(handle), 0) }
        let photo = try PhotoPrep.prepare(
            PhotoPrep.samplePNG(width: 64, height: 48, red: 0.2, alpha: 1))
        let admitted = NativeTestSignal("Photo write admitted")
        let release = DispatchSemaphore(value: 0)
        let worker = Task {
            await IdentityIO.performBlocking {
                Result {
                    try store.withIdentity(.slot(id), peerId: peer) { dir, key in
                        try profilePhotoRows(dir: dir, key: key) { dir, key in
                            admitted.signal()
                            guard release.wait(timeout: .now() + 10) == .success else {
                                throw CocoaError(.fileWriteUnknown)
                            }
                            _ = try addProfilePhoto(
                                dir: dir, key: key, full: photo.full, thumbnail: photo.thumbnail)
                            return try profilePhotoSnapshot(dir: dir, key: key)
                        }
                    }
                }
            }
        }
        let ready = await admitted.wait()
        worker.cancel()
        if ready {
            let attempt = flock(handle, LOCK_EX | LOCK_NB)
            let error = errno
            if attempt == 0 { _ = flock(handle, LOCK_UN) }
            XCTAssertEqual(attempt, -1)
            XCTAssertEqual(error, EWOULDBLOCK)
        }
        release.signal()
        let result = await worker.value
        XCTAssertTrue(ready, "The photo write must reach native admission")
        XCTAssertEqual(try result.get().hashes.count, 1)
        _ = try store.remove(id, confirming: peer)
    }

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
