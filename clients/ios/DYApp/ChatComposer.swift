import SwiftUI

/// Presentation ownership only: native submission results decide whether anything committed.
@MainActor
final class ChatComposer: ObservableObject {
    struct Submission {
        let id = UUID()
        let opening: UUID
        let revision: UUID
        let editRevision: UUID
        let text: String
        let target: ChatMessage?
    }

    struct Edit {
        let revision: UUID
        let draftRevision: UUID
    }

    @Published var draft = "" { didSet { revision = UUID() } }
    @Published private(set) var editing: Data?
    @Published private(set) var sending = false
    @Published var status: ChatStatus?
    private var held = ""
    private var revision = UUID()
    private var editRevision = UUID()
    private(set) var opening = UUID()
    private var pending: UUID?

    var edit: Edit { Edit(revision: editRevision, draftRevision: revision) }

    /// A return to the same peer is a distinct opening, even when the owner is unchanged.
    func retire() {
        opening = UUID()
        pending = nil
        sending = false
    }

    func edit(_ message: ChatMessage) {
        if editing == nil { held = draft }
        editing = message.threadKey
        editRevision = UUID()
        draft = message.text ?? ""
        status = nil
    }

    func endEditing() {
        guard editing != nil else { return }
        draft = held
        held = ""
        editing = nil
        editRevision = UUID()
    }

    /// A delete/clear completion cannot restore held text over a successor edit or newer typing.
    func endEditing(ifUnchanged edit: Edit) {
        guard edit.revision == editRevision, edit.draftRevision == revision else { return }
        endEditing()
    }

    func begin(state: ChatState?, messages: [ChatMessage]) -> Submission? {
        guard !sending, chatSendable(state, draft) else { return nil }
        let target = messages.first { $0.threadKey == editing && !$0.deleted }
        // Losing an edit target never turns the edit into a new message.
        guard editing == nil || target != nil else { return nil }
        let submission = Submission(
            opening: opening, revision: revision, editRevision: editRevision,
            text: draft, target: target)
        pending = submission.id
        sending = true
        status = nil
        return submission
    }

    func complete(_ submission: Submission, result: Result<ChatSubmission, Error>) {
        guard opening == submission.opening, pending == submission.id else { return }
        pending = nil
        sending = false
        guard editRevision == submission.editRevision else { return }
        status = chatSendStatus(
            result, queued: submission.target == nil ? "chat.queued" : "chat.editQueued",
            failed: submission.target == nil ? "chat.sendFailed" : "chat.editFailed")
        guard case .success = result, revision == submission.revision else { return }
        if submission.target != nil { endEditing() } else { draft = "" }
    }
}
