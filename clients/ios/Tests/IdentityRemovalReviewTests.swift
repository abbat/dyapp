@testable import DYApp
import XCTest

final class IdentityRemovalReviewTests: XCTestCase {
    func testDistinctProfilesDoNotCountAsAnotherCopyAndReviewDoesNotMutateSelection() throws {
        let root = try identitySlotTestRoot()
        let store = IdentitySlotStore(root: root, legacyStore: .forSlot(UUID()))
        let first = try store.create()
        let second = try store.create()
        XCTAssertNotEqual(first.peerId, second.peerId)
        let pointer = root.appendingPathComponent("active-profile")
        let selected = try Data(contentsOf: pointer)
        let key = root.appendingPathComponent("profiles")
            .appendingPathComponent(first.slot.id.uuidString.lowercased())
            .appendingPathComponent("identity.key")
        let wrapped = try Data(contentsOf: key)
        let review = try store.removalReview(.slot(first.slot.id), peerId: first.peerId)
        XCTAssertTrue(review.allowed)
        XCTAssertFalse(review.hasOtherCopy, "Another identity is not another copy of this key")
        XCTAssertThrowsError(try store.removalReview(.slot(first.slot.id), peerId: second.peerId))
        XCTAssertEqual(try Data(contentsOf: pointer), selected)
        XCTAssertEqual(try Data(contentsOf: key), wrapped)
        XCTAssertEqual(try store.open(.slot(first.slot.id)).peerId(), first.peerId)
    }
}
