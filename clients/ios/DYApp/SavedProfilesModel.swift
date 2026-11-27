import Foundation

/// Local IDs only: explicit Open owns any later public-profile or media request.
@MainActor
final class SavedProfilesModel: ObservableObject {
    struct Request: Sendable {
        let owner: UUID
        let id: UUID
        let removing: String?
    }

    @Published private(set) var peers: [String]?
    @Published private(set) var busy = false
    @Published private(set) var status: String?
    @Published private(set) var owner = UUID()
    private let feed: ChatFeed
    private let context: ChatFeed.Context?
    private var pending: UUID?
    private var active = false

    init(feed: ChatFeed) {
        self.feed = feed
        context = feed.context
    }

    func activate() {
        guard !active else { return }
        owner = UUID()
        active = true
    }

    func retire() {
        active = false
        owner = UUID()
        pending = nil
        busy = false
    }

    func current(_ expected: UUID) -> Bool {
        active && owner == expected && context.map { feed.isCurrent($0) } == true
    }

    func begin(owner expected: UUID, removing: String?) -> Request? {
        guard current(expected), !busy else { return nil }
        if let removing, peers?.contains(removing) != true { return nil }
        let request = Request(owner: owner, id: UUID(), removing: removing)
        pending = request.id
        busy = true
        status = nil
        return request
    }

    func complete(_ result: Result<[String], Error>, for request: Request) {
        guard current(request.owner), pending == request.id else { return }
        pending = nil
        busy = false
        switch result {
        case .success(let values):
            peers = values

        case .failure:
            status = request.removing == nil ? "favorites.listFailed" : "favorites.changeFailed"
        }
    }

    func refresh(owner expected: UUID, removing: String? = nil) async {
        guard let context, let request = begin(owner: expected, removing: removing) else { return }
        let result = await feed.call(context) { session in
            if let peer = request.removing { try session.setFavorite(peerId: peer, saving: false) }
            return try session.favoritePeers()
        }
        complete(result, for: request)
    }

    func canOpen(_ peer: String, owner expected: UUID) -> Bool {
        current(expected) && !busy && peers?.contains(peer) == true
    }
}
