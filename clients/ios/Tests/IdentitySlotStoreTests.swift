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

    func testNetworkDirectoryFollowsSelectedIdentityWithoutCreatingIt() throws {
        let root = try root()
        let id = UUID()
        let legacy = IdentityCandidate(location: .legacy, state: .ready)
        let slot = IdentityCandidate(location: .slot(id), state: .ready)
        XCTAssertEqual(RustBridge.networkDirectory(root: root, candidate: legacy), root)
        let expected = root.appendingPathComponent("profiles", isDirectory: true)
            .appendingPathComponent(id.uuidString.lowercased(), isDirectory: true)
        XCTAssertEqual(RustBridge.networkDirectory(root: root, candidate: slot), expected)
        XCTAssertFalse(FileManager.default.fileExists(atPath: expected.path))
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

    func testLegacyRootIdentityReopensWithItsOriginalPeerID() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let original = IdentityKey.generate()
        try original.wrap(store: legacyStore).write(
            to: path.appendingPathComponent("identity.key"), options: .atomic
        )
        let slots = IdentitySlotStore(root: path, legacyStore: legacyStore)
        let ready = IdentityCandidate(location: .legacy, state: .ready)
        XCTAssertEqual(try slots.resolve(), .ready(ready, original.peerId()))
        XCTAssertEqual(try slots.select(.legacy), original.peerId())
        let reopened = IdentitySlotStore(root: path, legacyStore: legacyStore)
        XCTAssertEqual(try reopened.resolve(), .ready(ready, original.peerId()))
        XCTAssertEqual(try reopened.open(.legacy).peerId(), original.peerId())
    }

    func testTwoSlotsCanBeSelectedAndReopened() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let slots = IdentitySlotStore(root: path, legacyStore: legacyStore)
        let first = try slots.create()
        let second = try slots.create()
        guard case .picker(let choices) = try slots.resolve() else {
            return XCTFail("two unselected slots should show a picker")
        }
        XCTAssertEqual(choices.count, 2)
        XCTAssertEqual(try slots.select(.slot(first.slot.id)), first.peerId)
        XCTAssertEqual(
            try slots.resolve(),
            .ready(IdentityCandidate(location: .slot(first.slot.id), state: .ready), first.peerId)
        )
        XCTAssertEqual(try slots.select(.slot(second.slot.id)), second.peerId)
        let reopened = IdentitySlotStore(root: path, legacyStore: legacyStore)
        XCTAssertEqual(
            try reopened.resolve(),
            .ready(IdentityCandidate(location: .slot(second.slot.id), state: .ready), second.peerId)
        )
    }

    func testInvalidPointerFallsBackWithoutChangingAKey() throws {
        let path = try root()
        let slots = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let first = try slots.create()
        let pointer = path.appendingPathComponent("active-profile")
        try Data("../escape\n".utf8).write(to: pointer)
        XCTAssertEqual(
            try slots.resolve(),
            .ready(IdentityCandidate(location: .slot(first.slot.id), state: .ready), first.peerId)
        )
        XCTAssertEqual(try Data(contentsOf: pointer), Data("../escape\n".utf8))
        let second = try slots.create()
        guard case .picker(let choices) = try slots.resolve() else {
            return XCTFail("invalid pointer with two slots should show a picker")
        }
        XCTAssertEqual(choices.count, 2)
        XCTAssertEqual(try slots.open(.slot(second.slot.id)).peerId(), second.peerId)
    }

    func testExplicitNoSelectionDoesNotAutoOpenSoleSurvivor() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let store = IdentitySlotStore(root: path, legacyStore: legacyStore)
        let only = try store.create()
        try store.select(.slot(only.slot.id))
        let pointer = path.appendingPathComponent("active-profile")
        try Data("none\n".utf8).write(to: pointer, options: .atomic)
        let reopened = IdentitySlotStore(root: path, legacyStore: legacyStore)
        XCTAssertEqual(try reopened.resolve(), .picker([
            IdentityCandidate(location: .slot(only.slot.id), state: .ready),
        ]))
        XCTAssertEqual(try reopened.open(.slot(only.slot.id)).peerId(), only.peerId)
        XCTAssertEqual(try Data(contentsOf: pointer), Data("none\n".utf8))
        XCTAssertEqual(try reopened.select(.slot(only.slot.id)), only.peerId)
        XCTAssertEqual(try reopened.resolve(),
                       .ready(IdentityCandidate(location: .slot(only.slot.id), state: .ready), only.peerId))
    }

    func testExplicitNoSelectionKeepsMissingLegacyKeyVisible() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let store = IdentitySlotStore(root: path, legacyStore: legacyStore)
        try Data("none\n".utf8).write(to: path.appendingPathComponent("active-profile"))
        XCTAssertEqual(try store.resolve(), .setup)
        _ = try IdentityKey.generate().wrap(store: legacyStore)
        XCTAssertEqual(try store.resolve(), .recovery(.legacy, .missing))
    }

    func testDeletingTombstoneRemainsVisibleWithoutDirectory() throws {
        let path = try root()
        let profiles = path.appendingPathComponent("profiles")
        try FileManager.default.createDirectory(at: profiles, withIntermediateDirectories: true)
        let id = UUID()
        let marker = profiles.appendingPathComponent("\(id.uuidString.lowercased()).deleting")
        try Data("deleting\n".utf8).write(to: marker)
        let store = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let candidate = IdentityCandidate(location: .slot(id), state: .deleting)
        XCTAssertEqual(try store.candidates(), [candidate])
        XCTAssertEqual(try store.resolve(), .recovery(.slot(id), .deleting))

        let folder = profiles.appendingPathComponent(id.uuidString.lowercased())
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        try Data("pending\n".utf8).write(to: folder.appendingPathComponent("state"))
        XCTAssertEqual(try store.candidates(), [candidate])
        try Data("wrong\n".utf8).write(to: marker, options: .atomic)
        XCTAssertEqual(try store.candidates(), [IdentityCandidate(location: .slot(id), state: .pending)])

        let otherID = UUID()
        let link = profiles.appendingPathComponent("\(otherID.uuidString.lowercased()).deleting")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: marker)
        try Data("deleting\n".utf8).write(to: marker, options: .atomic)
        XCTAssertEqual(try store.candidates(), [candidate])
    }

    func testConfirmedRemovalKeepsOtherSlotAndLeavesNoSelection() throws {
        let path = try root()
        let store = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let first = try store.create()
        let survivor = try store.create()
        let survivorFolder = path.appendingPathComponent("profiles")
            .appendingPathComponent(survivor.slot.id.uuidString.lowercased())
        let survivorKey = survivorFolder.appendingPathComponent("identity.key")
        let keyBytes = try Data(contentsOf: survivorKey)
        let draft = survivorFolder.appendingPathComponent("network-draft")
        try Data("survivor draft".utf8).write(to: draft)
        try store.select(.slot(first.slot.id))
        XCTAssertThrowsError(try store.remove(first.slot.id, confirming: survivor.peerId))
        XCTAssertEqual(try store.open(first.slot).peerId(), first.peerId)
        XCTAssertEqual(try store.resolve(), .ready(
            IdentityCandidate(location: .slot(first.slot.id), state: .ready), first.peerId
        ))

        try store.remove(first.slot.id, confirming: first.peerId)
        XCTAssertFalse(try PlatformKeyStore.forSlot(first.slot.id).containsWrappingKey())
        XCTAssertFalse(FileManager.default.fileExists(atPath: path.appendingPathComponent("profiles")
            .appendingPathComponent(first.slot.id.uuidString.lowercased()).path))
        XCTAssertEqual(try store.resolve(), .picker([
            IdentityCandidate(location: .slot(survivor.slot.id), state: .ready),
        ]))
        XCTAssertEqual(try store.open(survivor.slot).peerId(), survivor.peerId)
        XCTAssertEqual(try Data(contentsOf: survivorKey), keyBytes)
        XCTAssertEqual(try Data(contentsOf: draft), Data("survivor draft".utf8))
    }

    func testRemovalRefusesDamagedSlotAndPreservesLegacyAccount() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let legacy = IdentityKey.generate()
        let legacyFile = path.appendingPathComponent("identity.key")
        try legacy.wrap(store: legacyStore).write(to: legacyFile, options: .atomic)
        let original = try Data(contentsOf: legacyFile)
        let store = IdentitySlotStore(root: path, legacyStore: legacyStore)
        let slot = try store.create()
        let state = path.appendingPathComponent("profiles")
            .appendingPathComponent(slot.slot.id.uuidString.lowercased())
            .appendingPathComponent("state")
        try Data("invalid\n".utf8).write(to: state, options: .atomic)
        XCTAssertThrowsError(try store.remove(slot.slot.id, confirming: slot.peerId))
        XCTAssertTrue(try PlatformKeyStore.forSlot(slot.slot.id).containsWrappingKey())
        XCTAssertEqual(try Data(contentsOf: legacyFile), original)
        XCTAssertEqual(try store.open(.legacy).peerId(), legacy.peerId())
    }

    func testRemovalRefusesUnknownContentBeforeErasingKey() throws {
        let path = try root()
        let store = IdentitySlotStore(root: path)
        let slot = try store.create()
        let folder = path.appendingPathComponent("profiles")
            .appendingPathComponent(slot.slot.id.uuidString.lowercased())
        try Data("keep".utf8).write(to: folder.appendingPathComponent("unknown"))
        XCTAssertThrowsError(try store.remove(slot.slot.id, confirming: slot.peerId))
        XCTAssertEqual(try store.open(slot.slot).peerId(), slot.peerId)
        XCTAssertFalse(FileManager.default.fileExists(atPath: path.appendingPathComponent("profiles")
            .appendingPathComponent("\(slot.slot.id.uuidString.lowercased()).deleting").path))
    }

    func testRemovalResumesAfterKeychainEraseAndMissingDirectory() throws {
        let path = try root()
        let store = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let slot = try store.create()
        let profiles = path.appendingPathComponent("profiles")
        let tombstone = profiles.appendingPathComponent("\(slot.slot.id.uuidString.lowercased()).deleting")
        try Data("deleting\n".utf8).write(to: tombstone)
        try PlatformKeyStore.eraseWrappingKey(forSlot: slot.slot.id)
        let folder = profiles.appendingPathComponent(slot.slot.id.uuidString.lowercased())
        try FileManager.default.removeItem(at: folder)
        try store.remove(slot.slot.id, confirming: slot.peerId)
        XCTAssertEqual(try store.candidates(), [])
        XCTAssertFalse(FileManager.default.fileExists(atPath: tombstone.path))
    }

    func testRemovalFromPickerDoesNotAutoSelectSoleSurvivor() throws {
        let store = IdentitySlotStore(root: try root(), legacyStore: PlatformKeyStore.forSlot(UUID()))
        let removed = try store.create()
        let survivor = try store.create()
        try store.remove(removed.slot.id, confirming: removed.peerId)
        XCTAssertEqual(try store.resolve(), .picker([
            IdentityCandidate(location: .slot(survivor.slot.id), state: .ready),
        ]))
    }

    func testMissingLegacyAndIncompleteSlotNeverCreateAReplacement() throws {
        let path = try root()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let slots = IdentitySlotStore(root: path, legacyStore: legacyStore)
        XCTAssertEqual(try slots.resolve(), .setup)

        let key = IdentityKey.generate()
        _ = try key.wrap(store: legacyStore)
        XCTAssertEqual(try slots.resolve(), .recovery(.legacy, .missing))
        let pointer = path.appendingPathComponent("active-profile")
        try Data("legacy-primary\n".utf8).write(to: pointer)
        XCTAssertEqual(try slots.resolve(), .recovery(.legacy, .missing))

        let id = UUID()
        let folder = path.appendingPathComponent("profiles")
            .appendingPathComponent(id.uuidString.lowercased())
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("pending\n".utf8).write(to: folder.appendingPathComponent("state"))
        try Data("\(id.uuidString.lowercased())\n".utf8).write(to: pointer)
        XCTAssertEqual(try slots.resolve(), .recovery(.slot(id), .incomplete))
        XCTAssertFalse(try PlatformKeyStore.forSlot(id).containsWrappingKey())
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
