@testable import DYApp
import XCTest

final class NativeSlotKeysTests: XCTestCase {
    func testMissingExistingWrapAndInvalidBlobNeverCreateAReplacement() throws {
        let id = UUID().uuidString.lowercased()
        let keys = NativeSlotKeys()
        let key = try IdentityKey.generate()
        defer { try? keys.erase(slotId: id) }
        XCTAssertFalse(try keys.contains(slotId: id))
        XCTAssertNil(try keys.openWrapped(slotId: id, wrapped: Data([0])))
        XCTAssertThrowsError(try keys.wrapExisting(slotId: id, key: key))
        XCTAssertFalse(try keys.contains(slotId: id))
        let wrapped = try keys.wrap(slotId: id, key: key)
        XCTAssertThrowsError(try keys.openWrapped(slotId: id, wrapped: Data([0])))
        XCTAssertEqual(try keys.openWrapped(slotId: id, wrapped: wrapped)?.peerId(), key.peerId())
    }

    func testFreshWrapRefusesCollisionAndExistingWrapRetainsOldCredential() throws {
        let uuid = UUID()
        let id = uuid.uuidString.lowercased()
        let keys = NativeSlotKeys()
        let original = try IdentityKey.generate()
        let successor = try IdentityKey.generate()
        defer { try? keys.erase(slotId: id) }
        let wrapped = try keys.wrap(slotId: id, key: original)
        XCTAssertThrowsError(try keys.wrap(slotId: id, key: successor))
        XCTAssertEqual(
            try IdentityKey.unwrap(store: PlatformKeyStore.forSlot(uuid), wrapped: wrapped)
                .peerId(), original.peerId())
        let rewrapped = try keys.wrapExisting(slotId: id, key: original)
        XCTAssertEqual(
            try keys.openWrapped(slotId: id, wrapped: rewrapped)?.peerId(), original.peerId())
        XCTAssertEqual(
            try keys.openWrapped(slotId: id, wrapped: wrapped)?.peerId(), original.peerId())
        try keys.erase(slotId: id)
        XCTAssertFalse(try keys.contains(slotId: id))
        XCTAssertNil(try keys.openWrapped(slotId: id, wrapped: wrapped))
        XCTAssertThrowsError(try keys.wrapExisting(slotId: id, key: original))
    }

    func testLegacyAccountReopensWithoutMovingOrReplacingItsCredential() throws {
        let keys = NativeSlotKeys()
        let key = try IdentityKey.generate()
        let wrapped = try key.wrap(store: PlatformKeyStore.shared)
        XCTAssertTrue(try keys.contains(slotId: "legacy-primary"))
        XCTAssertEqual(
            try keys.openWrapped(slotId: "legacy-primary", wrapped: wrapped)?.peerId(), key.peerId()
        )
        let rewrapped = try keys.wrapExisting(slotId: "legacy-primary", key: key)
        XCTAssertEqual(
            try IdentityKey.unwrap(store: PlatformKeyStore.shared, wrapped: rewrapped).peerId(),
            key.peerId())
        XCTAssertThrowsError(try keys.erase(slotId: "legacy-primary"))
        XCTAssertEqual(
            try keys.openWrapped(slotId: "legacy-primary", wrapped: wrapped)?.peerId(), key.peerId()
        )
    }

    func testMalformedCredentialIdentifiersAreErrorsNotMissingAccounts() {
        let keys = NativeSlotKeys()
        for id in ["", "../identity-wrap", "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA"] {
            XCTAssertThrowsError(try keys.contains(slotId: id))
            XCTAssertThrowsError(try keys.openWrapped(slotId: id, wrapped: Data()))
        }
    }

    func testRealCommonRegistryCallsTheSwiftAdapterAndReopensTheOldLayout() throws {
        let root = try identitySlotTestRoot()
        let keys = NativeSlotKeys()
        let registry = MobileIdentityRegistry(root: root.path, layout: .ios, keys: keys)
        let created = try registry.create()
        defer { _ = try? registry.remove(id: created.slot.id, confirmedPeer: created.key.peerId()) }
        let id = try XCTUnwrap(UUID(uuidString: created.slot.id))
        let folder = root.appendingPathComponent("profiles").appendingPathComponent(created.slot.id)
        let wrapped = try Data(contentsOf: folder.appendingPathComponent("identity.key"))
        XCTAssertEqual(
            try IdentityKey.unwrap(store: PlatformKeyStore.forSlot(id), wrapped: wrapped).peerId(),
            created.key.peerId())
        XCTAssertEqual(
            try Data(contentsOf: folder.appendingPathComponent("state")), Data("ready\n".utf8))
        XCTAssertEqual(
            try Data(contentsOf: root.appendingPathComponent("active-profile")),
            Data("\(created.slot.id)\n".utf8))
        let reopened = MobileIdentityRegistry(root: root.path, layout: .ios, keys: NativeSlotKeys())
        guard case let .ready(slot, key) = try reopened.resolve() else {
            return XCTFail("The actual exported registry must reopen the same ready identity")
        }
        XCTAssertEqual(slot.id, created.slot.id)
        XCTAssertEqual(key.peerId(), created.key.peerId())
    }

    func testIsolatedLegacyRootRetainsItsOriginalCredentialBinding() throws {
        let root = try identitySlotTestRoot()
        let account = UUID()
        let legacy = PlatformKeyStore.forSlot(account)
        defer { try? PlatformKeyStore.eraseWrappingKey(forSlot: account) }
        let owner = try IdentityKey.generate()
        let blob = try owner.wrap(store: legacy)
        try blob.write(to: root.appendingPathComponent("identity.key"), options: .atomic)
        let registry = MobileIdentityRegistry(
            root: root.path, layout: .ios, keys: NativeSlotKeys(legacy: legacy))
        guard case let .ready(slot, key) = try registry.resolve() else {
            return XCTFail("An isolated legacy root must use its injected account")
        }
        XCTAssertEqual(slot.id, "legacy-primary")
        XCTAssertEqual(key.peerId(), owner.peerId())
        XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent("identity.key")), blob)
    }
}
