@testable import DYApp
import XCTest

final class NativeContactNoteTests: XCTestCase {
    func testExactPrivateTextReopensWithoutLikeAndRemainsIsolatedAndClearableWhenBlocked() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let owner = try store.create()
        let other = try store.create()
        // String equality folds canonical equivalents; UTF-8 comparisons prove exact preservation.
        let text = "Case\nPrivate note\n\u{1f600}"
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertEqual(try session.contactNote(peerId: other.peerId), "")
            try session.setContactNote(peerId: other.peerId, note: text)
            XCTAssertEqual(
                Array(try session.contactNote(peerId: other.peerId).utf8), Array(text.utf8))
            XCTAssertFalse(try session.likes(peerId: other.peerId).ours)
            XCTAssertTrue(try session.chatRows().isEmpty)
            XCTAssertTrue(try session.likedYou().isEmpty)
        }
        try withChats(store, id: other.slot.id, peer: other.peerId) { session in
            XCTAssertEqual(try session.contactNote(peerId: owner.peerId), "")
            XCTAssertTrue(try session.chatRows().isEmpty)
        }
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertEqual(
                Array(try session.contactNote(peerId: other.peerId).utf8), Array(text.utf8))
            _ = try session.block(peerId: other.peerId)
            try session.setContactNote(peerId: other.peerId, note: "blocked but private")
            XCTAssertEqual(try session.contactNote(peerId: other.peerId), "blocked but private")
            try session.setContactNote(peerId: other.peerId, note: "")
        }
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertEqual(try session.contactNote(peerId: other.peerId), "")
        }
    }

    func testNativeUnicodeScalarLimitAndInvalidTextRefusalPreservePriorBytes() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let owner = try store.create()
        let other = try store.create()
        let maximum = String(repeating: "\u{1f600}", count: 2_000)
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            try session.setContactNote(peerId: other.peerId, note: maximum)
            for invalid in [maximum + "\u{1f600}", "hidden\u{202e}", "e\u{301}", "tab\t"] {
                XCTAssertThrowsError(
                    try session.setContactNote(peerId: other.peerId, note: invalid)
                ) { error in
                    guard case ContactNoteError.InvalidText = error else {
                        return XCTFail("Expected typed native text refusal")
                    }
                }
                XCTAssertEqual(
                    Array(try session.contactNote(peerId: other.peerId).utf8), Array(maximum.utf8))
            }
        }
    }

    private func withChats(
        _ store: IdentitySlotStore, id: UUID, peer: String,
        _ body: (ChatSession) throws -> Void
    ) throws {
        let session = try store.registry.openChats(
            id: id.uuidString.lowercased(), expectedPeer: peer,
            store: PlatformKeyStore.forSlot(id).requiringExistingKey())
        try withExtendedLifetime(session) { try body(session) }
    }
}
