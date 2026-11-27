import SwiftUI

/// This draft is private contact data, never part of ProfilePublicRows or a message composer.
struct ProfileNoteBlock: View {
    @ObservedObject var page: ProfilePage
    @ScaledMetric(relativeTo: .body) private var editorHeight: CGFloat = 96

    var body: some View {
        let expected = page.opening
        Text("notes.title").font(.headline)
        TextEditor(
            text: Binding(
                get: { page.note.draft },
                set: { page.editNote($0, opening: expected) }
            )
        )
        .frame(minHeight: editorHeight)
        .disabled(!page.note.editable)
        .accessibilityLabel(Text("notes.title"))
        .accessibilityIdentifier("view-note-text")
        Text("notes.hint").font(.caption)
        if let status = page.note.status {
            Text(LocalizedStringKey(status)).font(.caption)
                .accessibilityIdentifier("view-note-status")
        }
        if page.note.saved == nil {
            Button("notes.retry") {
                Task { await page.resolveNote(.read, opening: expected) }
            }
            .disabled(page.note.busy)
            .accessibilityIdentifier("view-note-retry")
        } else {
            Button("notes.save") {
                Task { await page.resolveNote(.save, opening: expected) }
            }
            .disabled(!page.note.canSave)
            .accessibilityIdentifier("view-note-save")
            Button("notes.clear") {
                Task { await page.resolveNote(.clear, opening: expected) }
            }
            .disabled(!page.note.canClear)
            .accessibilityIdentifier("view-note-clear")
        }
    }
}
