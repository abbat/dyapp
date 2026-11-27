@testable import DYApp
import XCTest

@MainActor
final class ProfileSharingTests: XCTestCase {
    func testNativeCommitmentDeterminesShareStatus() {
        XCTAssertEqual(
            profileShareStatus(.success(.accepted(id: Data([1])))),
            ProfilePageStatus(key: "view.shared", details: nil))
        XCTAssertEqual(
            profileShareStatus(.success(.queued(id: Data([1]), detail: "offline"))),
            ProfilePageStatus(key: "chat.queued", details: nil))
        let refused = profileShareStatus(.failure(CoreError.Failed(message: "Queued")))
        XCTAssertEqual(refused.key, "chat.sendFailed")
        XCTAssertNotNil(refused.details, "Error wording cannot imply native commitment")
    }

    func testOnlyDistinctOpenTargetsAreOffered() {
        let states: [ChatState] = [.none, .requestSent, .requestReceived, .open, .closed]
        let rows = states.enumerated().map { index, state in
            ChatRow(
                peerId: "peer-\(index)", state: state, lastMessage: nil,
                preview: ChatListPreview(body: .empty, forwarded: false, sentAtMs: nil),
                unreadCount: 0, muted: false)
        }
        XCTAssertEqual(profileShareTargets(rows, sharedPeer: "other").map(\.peerId), ["peer-3"])
        XCTAssertTrue(profileShareTargets(rows, sharedPeer: "peer-3").isEmpty)
        XCTAssertTrue(profileShareTargets([], sharedPeer: "other").isEmpty)
    }

    func testRetiredPageAndOwnerSwitchCannotScheduleAShare() async {
        let feed = ChatFeed()
        let identity = ChatIdentity(
            candidate: IdentityCandidate(location: .slot(UUID()), state: .ready), peerId: "owner")
        feed.activate(identity)
        let retired = ProfilePage(peer: "owner", known: nil, feed: feed)
        let opening = retired.opening
        retired.retire()
        retired.activate()
        let retiredSent = await retired.share(to: "target", opening: opening)
        XCTAssertFalse(retiredSent)
        XCTAssertFalse(retired.sending)
        let oldOwner = ProfilePage(peer: "owner", known: nil, feed: feed)
        feed.deactivate()
        feed.activate(identity)
        let oldSent = await oldOwner.share(to: "target", opening: oldOwner.opening)
        XCTAssertFalse(oldSent)
        XCTAssertFalse(oldOwner.sending)
        XCTAssertNil(oldOwner.status)
    }
}
