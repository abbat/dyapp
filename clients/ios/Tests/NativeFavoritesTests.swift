@testable import DYApp
import XCTest

final class NativeFavoritesTests: XCTestCase {
    func testPrivateFavoritePersistsAcrossReopenAndDoesNotCreateALikeOrChat() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let owner = try store.create()
        let other = try store.create()
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertFalse(try session.favoriteState(peerId: other.peerId))
            try session.setFavorite(peerId: other.peerId, saving: true)
            XCTAssertEqual(try session.favoritePeers(), [other.peerId])
            XCTAssertFalse(try session.likes(peerId: other.peerId).ours)
            XCTAssertTrue(try session.chatRows().isEmpty)
            XCTAssertTrue(try session.likedYou().isEmpty)
        }
        try withChats(store, id: other.slot.id, peer: other.peerId) { session in
            XCTAssertTrue(try session.favoritePeers().isEmpty)
            XCTAssertTrue(try session.chatRows().isEmpty)
            XCTAssertTrue(try session.likedYou().isEmpty)
        }
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertTrue(try session.favoriteState(peerId: other.peerId))
            try session.setFavorite(peerId: other.peerId, saving: false)
            XCTAssertTrue(try session.favoritePeers().isEmpty)
            XCTAssertFalse(try session.likes(peerId: other.peerId).ours)
        }
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertFalse(try session.favoriteState(peerId: other.peerId))
        }
    }

    func testOwnAndInvalidTargetsAreTypedRefusalsWithoutSavedRows() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let owner = try store.create()
        try withChats(store, id: owner.slot.id, peer: owner.peerId) { session in
            XCTAssertThrowsError(try session.setFavorite(peerId: owner.peerId, saving: true)) {
                error in
                guard case FavoriteError.OwnPeer = error else {
                    return XCTFail("Expected own-target refusal")
                }
            }
            XCTAssertThrowsError(try session.setFavorite(peerId: "invalid", saving: true)) {
                error in
                guard case FavoriteError.InvalidPeer = error else {
                    return XCTFail("Expected invalid-target refusal")
                }
            }
            XCTAssertTrue(try session.favoritePeers().isEmpty)
            XCTAssertTrue(try session.chatRows().isEmpty)
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
