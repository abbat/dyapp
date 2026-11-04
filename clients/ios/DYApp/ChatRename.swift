import SwiftUI

/// Names a peer on this device; empty takes the name off. A sheet, as a field in an alert needs
/// macOS 13 and iOS 16.
struct ChatRenameSheet: View {
    @Environment(\.dismiss) private var dismiss
    @State private var name: String
    @State private var saving = false
    @State private var failure: ChatStatus?
    private let save: @MainActor (String) async -> ChatStatus?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("rename.title").font(.headline)
            TextField("rename.label", text: $name)
                .disabled(saving)
                .onSubmit(submit)
                .accessibilityIdentifier("chat-rename-input")
            Text("rename.hint").font(.caption)
            if let failure {
                Text(verbatim: String(format: NSLocalizedString(failure.key, comment: ""), failure.details ?? ""))
                    .font(.caption)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("chat-rename-error")
            }
            HStack {
                Spacer()
                Button("search.cancel") { dismiss() }.accessibilityIdentifier("chat-rename-cancel")
                Button("rename.save", action: submit)
                    .disabled(saving)
                    .accessibilityIdentifier("chat-rename-save")
            }
        }
        .padding()
        .frame(minWidth: 320)
    }

    init(name: String, save: @escaping @MainActor (String) async -> ChatStatus?) {
        _name = State(initialValue: name)
        self.save = save
    }

    private func submit() {
        guard !saving else { return }
        saving = true
        Task {
            failure = await save(name)
            saving = false
            if failure == nil { dismiss() }
        }
    }
}
