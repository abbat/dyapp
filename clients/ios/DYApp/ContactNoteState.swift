import Foundation

/// Private editor presentation; validation, persistence and own-device sync belong to native core.
struct ContactNoteState {
    enum Action { case read, save, clear }
    struct Request: Sendable {
        let id: UUID
        let target: String?
    }

    private(set) var saved: String?
    private(set) var draft = ""
    private(set) var busy = false
    private(set) var status: String? = "notes.loading"
    private var pending: UUID?

    var editable: Bool { !busy && saved != nil }
    var canSave: Bool { editable && draft != saved }
    var canClear: Bool { editable && saved?.isEmpty == false }

    mutating func edit(_ text: String) {
        guard editable else { return }
        draft = text
        status = nil
    }

    mutating func begin(_ action: Action) -> Request? {
        guard !busy else { return nil }
        let target: String?
        switch action {
        case .read:
            guard saved == nil else { return nil }
            target = nil

        case .save:
            guard canSave else { return nil }
            target = draft

        case .clear:
            guard canClear else { return nil }
            target = ""
        }
        let request = Request(id: UUID(), target: target)
        pending = request.id
        busy = true
        status = target == nil ? "notes.loading" : "notes.saving"
        return request
    }

    mutating func complete(_ result: Result<String, Error>, for request: Request) {
        guard pending == request.id else { return }
        pending = nil
        busy = false
        switch result {
        case .success(let value):
            saved = value
            draft = value
            status = request.target == nil ? nil : value.isEmpty ? "notes.cleared" : "notes.saved"

        case .failure(let error):
            if case ContactNoteError.InvalidText = error {
                status = "notes.invalid"
            } else {
                status = request.target == nil ? "notes.loadFailed" : "notes.saveFailed"
            }
        }
    }
}
