@testable import DYApp
import XCTest

@MainActor
final class ChatOwnerTests: XCTestCase {
    func testSwitchAwayAndBackCannotReviveOldOwner() {
        let feed = ChatFeed()
        let first = identity(peer: "first")
        let second = identity(peer: "second")
        let old = feed.activate(first)
        XCTAssertTrue(feed.isCurrent(old))
        XCTAssertEqual(feed.activate(first).owner, old.owner, "An unchanged poll retains its owner")
        feed.activate(second)
        let current = feed.activate(first)
        XCTAssertNotEqual(old.owner, current.owner)
        XCTAssertFalse(feed.isCurrent(old))
        XCTAssertTrue(feed.isCurrent(current))
        XCTAssertEqual(old.identity.peerId, first.peerId)
        XCTAssertEqual(old.identity.candidate.location, first.candidate.location)
    }

    func testSamePeerUnderDifferentSlotStillReplacesTheOwner() {
        let feed = ChatFeed()
        let first = feed.activate(identity(peer: "same"))
        let second = feed.activate(identity(peer: "same"))
        XCTAssertFalse(feed.isCurrent(first))
        XCTAssertTrue(feed.isCurrent(second))
        feed.deactivate()
        XCTAssertFalse(feed.isCurrent(second))
        XCTAssertNil(feed.context)
        XCTAssertNil(feed.me)
    }

    func testRetiredCallRefusesBeforeExecutingNativeAction() async {
        let feed = ChatFeed()
        let old = feed.activate(identity(peer: "first"))
        feed.deactivate()
        let result = await feed.call(old) { _ in
            XCTFail("A retired UI request must never open a successor native session")
            return 0
        }
        guard case .failure = result else { return XCTFail("Expected a precommit refusal") }
    }

    private func identity(peer: String) -> ChatIdentity {
        ChatIdentity(
            candidate: IdentityCandidate(location: .slot(UUID()), state: .ready), peerId: peer)
    }
}
