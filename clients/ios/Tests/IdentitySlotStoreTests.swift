@testable import DYApp
import Foundation
import XCTest

final class IdentitySlotStoreTests: XCTestCase {
    private func root() throws -> URL {
        let path = FileManager.default.temporaryDirectory
            .appendingPathComponent("dyapp-slot-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: path, withIntermediateDirectories: false)
        return path
    }

    func testTwoSlotsReopenWithDistinctStablePeerIDs() throws {
        let store = IdentitySlotStore(root: try root())
        let first = try store.create()
        let second = try store.create()
        XCTAssertNotEqual(first.slot.id, second.slot.id)
        XCTAssertNotEqual(first.peerId, second.peerId)
        XCTAssertEqual(try store.open(first.slot).peerId(), first.peerId)
        XCTAssertEqual(try store.open(second.slot).peerId(), second.peerId)

        let reopened = try store.slots()
        XCTAssertEqual(reopened.count, 2)
        XCTAssertTrue(reopened.allSatisfy { $0.state == .ready })
        XCTAssertEqual(try store.open(reopened[0]).peerId(),
                       reopened[0].id == first.slot.id ? first.peerId : second.peerId)
    }

    func testPendingAndMalformedSlotsCannotGenerateReplacementKeys() throws {
        let path = try root()
        let id = UUID()
        let slot = path.appendingPathComponent("profiles", isDirectory: true)
            .appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
        try FileManager.default.createDirectory(at: slot, withIntermediateDirectories: true)
        try Data("pending\n".utf8).write(to: slot.appendingPathComponent("state"))
        let store = IdentitySlotStore(root: path)
        let pending = try XCTUnwrap(store.slots().first)
        XCTAssertEqual(pending.state, .pending)
        XCTAssertThrowsError(try store.open(pending))
        XCTAssertFalse(try PlatformKeyStore.forSlot(id).containsWrappingKey())

        try Data("unknown\n".utf8).write(to: slot.appendingPathComponent("state"))
        let damaged = try XCTUnwrap(store.slots().first)
        XCTAssertEqual(damaged.state, .damaged)
        XCTAssertThrowsError(try store.open(damaged))
        XCTAssertFalse(try PlatformKeyStore.forSlot(id).containsWrappingKey())
    }

    func testExistingKeychainAccountIsNeverReused() throws {
        let path = try root()
        let orphan = UUID()
        let next = UUID()
        let keyStore = PlatformKeyStore.forSlot(orphan)
        let secret = Data("orphaned wrapping key".utf8)
        let original = try keyStore.wrap(secret: secret)
        var calls = 0
        let slots = IdentitySlotStore(root: path) {
            calls += 1
            return calls == 1 ? orphan : next
        }
        XCTAssertEqual(try slots.create().slot.id, next)
        XCTAssertEqual(try keyStore.unwrap(wrapped: original), secret)
    }

    func testDirectoryCollisionIsNeverOverwritten() throws {
        let path = try root()
        let collision = UUID()
        let next = UUID()
        let first = IdentitySlotStore(root: path) { collision }
        let original = try first.create()
        var calls = 0
        let second = IdentitySlotStore(root: path) {
            calls += 1
            return calls == 1 ? collision : next
        }
        let another = try second.create()
        XCTAssertEqual(another.slot.id, next)
        XCTAssertEqual(try first.open(original.slot).peerId(), original.peerId)
        XCTAssertNotEqual(another.peerId, original.peerId)
    }
}
