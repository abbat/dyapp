import Foundation

/// What one action on a peer's page last told; failures carry details to select and copy.
struct ProfilePageStatus: Equatable {
    let key: String
    let details: String?
}

/// The status of a like that failed and stored nothing.
func likeFailureKey(_ error: Error) -> String {
    let text = String(describing: error)
    if text.contains("unpublished") { return "view.likeUnpublished" }
    if text.contains("notConnected") { return "view.likeNotConnected" }
    return "view.likeFailed"
}

/// One peer's page: the profile, the like state and the actions on it. The profile comes from the
/// opening list, or from the network when a chat opens the page.
@MainActor
final class ProfilePage: ObservableObject {
    @Published private(set) var found: FoundProfile?
    @Published private(set) var loading = false
    @Published private(set) var loadFailure: ProfilePageStatus?
    @Published private(set) var searched = false
    /// nil until read: every button that depends on it waits.
    @Published private(set) var likes: ChatLikes?
    @Published private(set) var likesFailure: ProfilePageStatus?
    @Published private(set) var sending = false
    @Published private(set) var status: ProfilePageStatus?
    let peer: String
    private let feed: ChatFeed

    init(peer: String, known: FoundProfile?, feed: ChatFeed) {
        self.peer = peer
        self.feed = feed
        found = known
    }

    func load() async {
        guard found == nil, !loading else { return }
        loading = true
        loadFailure = nil
        let peers = [peer]
        let result = await feed.call { try $0.profiles(peerIds: peers) }
        loading = false
        switch result {
        case let .success(profiles):
            found = profiles.first
            searched = true

        case let .failure(error):
            loadFailure = ProfilePageStatus(key: profileLoadKey(error), details: String(describing: error))
        }
    }

    /// Failures keep the last state; with none yet the buttons wait and the page says why.
    func readLikes() async {
        let peer = peer
        switch await feed.call({ try $0.likes(peerId: peer) }) {
        case let .success(read):
            likes = read
            likesFailure = nil

        case let .failure(error):
            if likes == nil {
                likesFailure = ProfilePageStatus(key: profileLoadKey(error), details: String(describing: error))
            }
        }
    }

    func like(_ liked: Bool) async {
        guard !sending else { return }
        sending = true
        status = nil
        let peer = peer
        let result = await feed.call { try $0.like(peerId: peer, liked: liked) }
        await feed.read()
        switch result {
        case let .success(read):
            likes = read

        case let .failure(error):
            await readLikes()
            // The core keeps a like it could not send and sends it with the next receive.
            status = likes?.ours == liked
                ? ProfilePageStatus(key: "view.likePending", details: nil)
                : ProfilePageStatus(key: likeFailureKey(error), details: String(describing: error))
        }
        sending = false
    }

    func pass(_ passed: Bool) async {
        guard !sending else { return }
        sending = true
        status = nil
        let peer = peer
        let result = await feed.call { try $0.setPassed(peerId: peer, passed: passed) }
        await feed.read()
        switch result {
        case let .success(read):
            likes = read
            status = ProfilePageStatus(key: passed ? "view.passed" : "view.unpassed", details: nil)

        case let .failure(error):
            status = ProfilePageStatus(key: "view.passFailed", details: String(describing: error))
        }
        sending = false
    }

    /// True when the block held and the page should close.
    func block() async -> Bool {
        guard !sending else { return false }
        sending = true
        status = nil
        let peer = peer
        let result = await feed.call { try $0.block(peerId: peer) }
        await feed.read()
        sending = false
        guard case let .failure(error) = result else { return true }
        status = ProfilePageStatus(key: "chat.blockFailed", details: String(describing: error))
        return false
    }
}
