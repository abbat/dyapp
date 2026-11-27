@testable import DYApp
import XCTest

@MainActor
final class SavedProfilesTests: XCTestCase {
    func testCloseReopenRefusesOldCompletionAndUnknownRowActions() throws {
        let feed = ChatFeed()
        feed.activate(identity())
        let model = SavedProfilesModel(feed: feed)
        model.activate()
        let oldOwner = model.owner
        let old = try XCTUnwrap(model.begin(owner: oldOwner, removing: nil))
        XCTAssertNil(model.begin(owner: oldOwner, removing: nil))
        model.retire()
        model.activate()
        let owner = model.owner
        let current = try XCTUnwrap(model.begin(owner: owner, removing: nil))
        model.complete(.success(["obsolete"]), for: old)
        XCTAssertNil(model.peers)
        XCTAssertTrue(model.busy)
        model.complete(.success(["saved"]), for: current)
        XCTAssertTrue(model.canOpen("saved", owner: owner))
        XCTAssertFalse(model.canOpen("saved", owner: oldOwner))
        XCTAssertFalse(model.canOpen("absent", owner: owner))
        XCTAssertNil(model.begin(owner: owner, removing: "absent"))
        let removal = try XCTUnwrap(model.begin(owner: owner, removing: "saved"))
        XCTAssertFalse(model.canOpen("saved", owner: owner))
        model.complete(.failure(CoreError.Failed(message: "storage")), for: removal)
        XCTAssertEqual(model.peers, ["saved"])
        XCTAssertEqual(model.status, "favorites.changeFailed")
    }

    func testSamePeerInAnotherSlotCannotReceiveOldListOrOpenAction() throws {
        let feed = ChatFeed()
        feed.activate(identity())
        let model = SavedProfilesModel(feed: feed)
        model.activate()
        let old = try XCTUnwrap(model.begin(owner: model.owner, removing: nil))
        feed.activate(identity())
        model.complete(.success(["obsolete"]), for: old)
        XCTAssertNil(model.peers)
        XCTAssertFalse(model.canOpen("obsolete", owner: old.owner))
        XCTAssertNil(model.begin(owner: old.owner, removing: nil))
        let successor = SavedProfilesModel(feed: feed)
        successor.activate()
        let current = try XCTUnwrap(successor.begin(owner: successor.owner, removing: nil))
        successor.complete(.failure(CoreError.Failed(message: "storage")), for: current)
        XCTAssertNil(successor.peers, "Failure must not become an empty collection")
        XCTAssertEqual(successor.status, "favorites.listFailed")
        let retry = try XCTUnwrap(successor.begin(owner: successor.owner, removing: nil))
        successor.complete(.success([]), for: retry)
        XCTAssertEqual(successor.peers, [])
        XCTAssertNil(successor.status)
    }

    private func identity() -> ChatIdentity {
        ChatIdentity(
            candidate: IdentityCandidate(location: .slot(UUID()), state: .ready), peerId: "same")
    }
}
