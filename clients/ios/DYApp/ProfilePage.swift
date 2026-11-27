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
    @Published private(set) var favorite = FavoriteActionState()
    @Published private(set) var note = ContactNoteState()
    let peer: String
    private let feed: ChatFeed
    private let context: ChatFeed.Context?
    private(set) var opening = UUID()
    private var active = true
    private var viewOpening: ProfileViewOpening?

    init(peer: String, known: FoundProfile?, feed: ChatFeed) {
        self.peer = peer
        self.feed = feed
        context = feed.context
        found = known
    }

    func retire() {
        favorite = FavoriteActionState()
        note = ContactNoteState()
        active = false
        viewOpening?.retire()
        viewOpening = nil
        opening = UUID()
    }

    func activate() {
        guard !active else { return }
        active = true
        loading = false
        sending = false
    }

    func current(_ expected: UUID) -> Bool {
        active && opening == expected && context.map { feed.isCurrent($0) } == true
    }

    func prepareViewDisclosure(opening expected: UUID) async {
        guard let context, current(expected), peer != context.identity.peerId,
            let worker = await feed.photos.use(context.identity),
            !Task.isCancelled, current(expected)
        else { return }
        viewOpening = try? worker.searcher.profileViewOpening(
            ownerPeer: context.identity.peerId, peerId: peer)
    }

    func discloseShownProfile(opening expected: UUID) async {
        guard let context, current(expected), found != nil, let viewOpening else { return }
        _ = await feed.call(context) { try $0.submitProfileView(opening: viewOpening) }
    }

    func load() async {
        guard found == nil, !loading, let context, current(opening) else { return }
        let expected = opening
        loading = true
        loadFailure = nil
        let peers = [peer]
        let result = await feed.call(context) { try $0.profiles(peerIds: peers) }
        guard current(expected) else { return }
        loading = false
        switch result {
        case let .success(profiles):
            found = profiles.first
            searched = true

        case let .failure(error):
            loadFailure = ProfilePageStatus(
                key: profileLoadKey(error), details: String(describing: error))
        }
    }

    /// Failures keep the last state; with none yet the buttons wait and the page says why.
    func readLikes() async {
        guard let context, current(opening) else { return }
        let expected = opening
        let peer = peer
        let result = await feed.call(context) { try $0.likes(peerId: peer) }
        guard current(expected) else { return }
        switch result {
        case let .success(read):
            likes = read
            likesFailure = nil

        case let .failure(error):
            if likes == nil {
                likesFailure = ProfilePageStatus(
                    key: profileLoadKey(error), details: String(describing: error))
            }
        }
    }

    /// A native private change never invokes Like or sends a message to this peer.
    func resolveFavorite(change: Bool, opening expected: UUID) async {
        guard let context, current(expected), let request = favorite.begin(change: change) else {
            return
        }
        let peer = peer
        let result = await feed.call(context) { session in
            if let target = request.target { try session.setFavorite(peerId: peer, saving: target) }
            return try session.favoriteState(peerId: peer)
        }
        guard current(expected) else { return }
        favorite.complete(result, for: request)
    }

    func editNote(_ text: String, opening expected: UUID) {
        guard current(expected) else { return }
        note.edit(text)
    }

    func resolveNote(_ action: ContactNoteState.Action, opening expected: UUID) async {
        guard let context, current(expected), let request = note.begin(action) else { return }
        let peer = peer
        let result = await feed.call(context) { session in
            if let target = request.target {
                try session.setContactNote(peerId: peer, note: target)
            }
            return try session.contactNote(peerId: peer)
        }
        guard current(expected) else { return }
        note.complete(result, for: request)
    }

    func like(_ liked: Bool) async {
        guard !sending, let context, current(opening) else { return }
        let expected = opening
        sending = true
        status = nil
        let peer = peer
        let result = await feed.call(context) { try $0.like(peerId: peer, liked: liked) }
        guard current(expected) else { return }
        await feed.read()
        guard current(expected) else { return }
        switch result {
        case let .success(read):
            likes = read

        case let .failure(error):
            await readLikes()
            guard current(expected) else { return }
            // The core keeps a like it could not send and sends it with the next receive.
            status =
                likes?.ours == liked
                ? ProfilePageStatus(key: "view.likePending", details: nil)
                : ProfilePageStatus(key: likeFailureKey(error), details: String(describing: error))
        }
        sending = false
    }

    func pass(_ passed: Bool) async {
        guard !sending, let context, current(opening) else { return }
        let expected = opening
        sending = true
        status = nil
        let peer = peer
        let result = await feed.call(context) { try $0.setPassed(peerId: peer, passed: passed) }
        guard current(expected) else { return }
        await feed.read()
        guard current(expected) else { return }
        switch result {
        case let .success(read):
            likes = read
            status = ProfilePageStatus(key: passed ? "view.passed" : "view.unpassed", details: nil)

        case let .failure(error):
            status = ProfilePageStatus(key: "view.passFailed", details: String(describing: error))
        }
        sending = false
    }

    /// Sends this page's link as a message in the chat with `target`.
    func share(to target: String, opening expected: UUID) async -> Bool {
        guard !sending, let context, current(expected) else { return false }
        let peer = peer
        sending = true
        status = nil
        let result = await feed.call(context) {
            try $0.submitProfileShare(peerId: target, sharedPeerId: peer)
        }
        guard current(expected) else { return false }
        status = profileShareStatus(result)
        await feed.read()
        guard current(expected) else { return false }
        sending = false
        guard case .success = result else { return false }
        return true
    }

    /// True when the block held and the page should close.
    func block() async -> Bool {
        guard !sending, let context, current(opening) else { return false }
        let expected = opening
        sending = true
        status = nil
        let failed = await feed.block(peer, in: context)
        guard current(expected) else { return false }
        sending = false
        guard let failed else { return true }
        status = ProfilePageStatus(key: failed.key, details: failed.details)
        return false
    }
}

/// Candidate rows are presentation only; submitProfileShare rechecks admission before staging.
func profileShareTargets(_ rows: [ChatRow], sharedPeer: String) -> [ChatRow] {
    rows.filter { $0.state == .open && $0.peerId != sharedPeer }
}

func profileShareStatus(_ result: Result<ChatSubmission, Error>) -> ProfilePageStatus {
    let status = chatSendStatus(result)
    return ProfilePageStatus(key: status?.key ?? "view.shared", details: status?.details)
}
