@testable import DYApp
import Foundation
import XCTest

extension IdentitySlotStoreTests {
    @objc
    func testUnreadableSlotCannotReplaceSelectedIdentity() throws {
        let path = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let selected = try store.create()
        let unreadable = try store.create()
        try store.select(.slot(selected.slot.id))
        let pointer = path.appendingPathComponent("active-profile")
        let savedPointer = try Data(contentsOf: pointer)
        let file = path.appendingPathComponent("profiles")
            .appendingPathComponent(unreadable.slot.id.uuidString.lowercased())
            .appendingPathComponent("identity.key")
        try Data("invalid wrapped key".utf8).write(to: file, options: .atomic)

        XCTAssertThrowsError(try store.select(.slot(unreadable.slot.id)))
        XCTAssertEqual(try Data(contentsOf: pointer), savedPointer)
        XCTAssertEqual(
            try store.resolve(),
            .ready(
                IdentityCandidate(location: .slot(selected.slot.id), state: .ready), selected.peerId
            ))
        XCTAssertTrue(try PlatformKeyStore.forSlot(unreadable.slot.id).containsWrappingKey())
        XCTAssertEqual(try store.candidates().count, 2)
    }

    @objc
    func testLostSelectedKeyIsConfirmedMissingWithoutReplacement() throws {
        let path = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let lost = try store.create()
        try store.select(.slot(lost.slot.id))
        let pointer = path.appendingPathComponent("active-profile")
        let savedPointer = try Data(contentsOf: pointer)
        try PlatformKeyStore.eraseWrappingKey(forSlot: lost.slot.id)

        XCTAssertEqual(try store.resolve(), .recovery(.slot(lost.slot.id), .missing))
        XCTAssertThrowsError(try store.select(.slot(lost.slot.id)))
        XCTAssertEqual(try Data(contentsOf: pointer), savedPointer)
        XCTAssertFalse(try PlatformKeyStore.forSlot(lost.slot.id).containsWrappingKey())
        XCTAssertEqual(
            try store.candidates(),
            [
                IdentityCandidate(location: .slot(lost.slot.id), state: .ready)
            ])
    }
}

extension IdentitySlotStoreTests {
    @objc
    func testProfileDraftsStayInTheirSlots() throws {
        let path = try identitySlotTestRoot()
        let slots = IdentitySlotStore(root: path, legacyStore: PlatformKeyStore.forSlot(UUID()))
        let first = try slots.create()
        let second = try slots.create()
        let firstCandidate = IdentityCandidate(location: .slot(first.slot.id), state: .ready)
        let secondCandidate = IdentityCandidate(location: .slot(second.slot.id), state: .ready)
        let firstDraft = ProfileDraftStore(
            root: path, candidate: firstCandidate, peerId: first.peerId)
        let secondDraft = ProfileDraftStore(
            root: path, candidate: secondCandidate, peerId: second.peerId)
        try firstDraft.save(["gender": 1])
        try secondDraft.save(["orientation": 2])
        XCTAssertNotEqual(firstDraft.file, secondDraft.file)
        XCTAssertEqual(try firstDraft.load(), ["gender": 1])
        XCTAssertEqual(try secondDraft.load(), ["orientation": 2])

    }
}

extension IdentitySlotStoreTests {
    @objc
    func testMissingLegacyAndIncompleteSlotNeverCreateAReplacement() throws {
        let path = try identitySlotTestRoot()
        let legacyStore = PlatformKeyStore.forSlot(UUID())
        let slots = IdentitySlotStore(root: path, legacyStore: legacyStore)
        XCTAssertEqual(try slots.resolve(), .setup)

        let key = try IdentityKey.generate()
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

    @objc
    func testDirectoryCollisionIsNeverOverwritten() throws {
        let path = try identitySlotTestRoot()
        let first = IdentitySlotStore(root: path)
        let original = try first.create()
        let keys = IdentityCollisionKeys(root: path, kind: .directory)
        let second = IdentitySlotStore(root: path, keys: keys)
        let another = try second.create()
        let collision = try XCTUnwrap(keys.collision.id)
        XCTAssertNotEqual(another.slot.id.uuidString.lowercased(), collision)
        XCTAssertEqual(try first.open(original.slot).peerId(), original.peerId)
        XCTAssertNotEqual(another.peerId, original.peerId)
        let folder = path.appendingPathComponent("profiles").appendingPathComponent(collision)
        XCTAssertEqual(
            try Data(contentsOf: folder.appendingPathComponent("state")), Data("pending\n".utf8))
        XCTAssertEqual(
            try Data(contentsOf: folder.appendingPathComponent("sentinel")),
            Data("preserve collision".utf8))
        XCTAssertFalse(try keys.contains(slotId: collision))
    }
}
