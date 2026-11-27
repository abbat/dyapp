import Foundation

/// Presentation only: native favorites own target validation, private storage and device sync.
struct FavoriteActionState {
    struct Request: Sendable {
        let id: UUID
        let target: Bool?
    }

    private(set) var saved: Bool?
    private(set) var busy = false
    private(set) var status: String?
    private var pending: UUID?

    var label: String {
        if busy { return saved == nil ? "favorites.loading" : "favorites.saving" }
        guard let saved else { return status == nil ? "favorites.loading" : "favorites.retry" }
        return saved ? "favorites.remove" : "favorites.add"
    }

    mutating func begin(change: Bool) -> Request? {
        guard !busy else { return nil }
        let request = Request(id: UUID(), target: change ? saved.map { !$0 } : nil)
        pending = request.id
        busy = true
        status = nil
        return request
    }

    mutating func complete(_ result: Result<Bool, Error>, for request: Request) {
        guard pending == request.id else { return }
        pending = nil
        busy = false
        switch result {
        case .success(let value):
            saved = value
            status = request.target == nil ? nil : value ? "favorites.saved" : "favorites.removed"

        case .failure(let error):
            if case FavoriteError.Blocked = error {
                status = "favorites.blocked"
            } else {
                status = request.target == nil ? "favorites.loadFailed" : "favorites.changeFailed"
            }
        }
    }
}
