import SwiftUI

/// Per-identity local editor state; a prior peer-named root draft is copied once on first read.
struct ProfileDraftStore {
    let root: URL
    let candidate: IdentityCandidate
    let peerId: String

    var file: URL {
        RustBridge.networkDirectory(root: root, candidate: candidate)
            .appendingPathComponent("profile-draft.json")
    }

    private var oldFile: URL { root.appendingPathComponent("profile-draft-\(peerId).json") }

    func load() throws -> [String: UInt32]? {
        let files = FileManager.default
        let current = file
        if files.fileExists(atPath: current.path) {
            return try JSONDecoder().decode([String: UInt32].self, from: Data(contentsOf: current))
        }
        guard files.fileExists(atPath: oldFile.path) else { return nil }
        let data = try Data(contentsOf: oldFile)
        let values = try JSONDecoder().decode([String: UInt32].self, from: data)
        try data.write(to: current, options: .atomic)
        return values
    }

    func save(_ values: [String: UInt32]) throws {
        try JSONEncoder().encode(values).write(to: file, options: .atomic)
    }
}

/// A local draft editor. The shared Rust core supplies every numeric choice.
struct ProfileDraftView: View {
    let candidate: IdentityCandidate
    let peerId: String
    @State private var values: [String: UInt32] = [:]
    @State private var status = ""
    @State private var readFailed = false

    private var draftStore: ProfileDraftStore {
        get throws { ProfileDraftStore(root: try RustBridge.directory(), candidate: candidate, peerId: peerId) }
    }

    var body: some View {
        Form {
            ForEach(profileScales(), id: \.field) { scale in
                Picker(LocalizedStringKey("profile.field.\(scale.key)"), selection: Binding(
                    get: { values[scale.key] ?? 0 },
                    set: { values[scale.key] = $0 }
                )) {
                    Text("profile.unset").tag(UInt32(0))
                    ForEach(scale.choices, id: \.code) { choice in
                        Text(LocalizedStringKey("profile.value.\(scale.key).\(choice.key)"))
                            .tag(choice.code)
                    }
                }
            }
            Button("profile.save") { save() }
                .disabled(readFailed)
                .accessibilityIdentifier("profile-save")
            if !status.isEmpty {
                Text(LocalizedStringKey(status))
            }
        }
        .navigationTitle("profile.title")
        .onAppear(perform: load)
    }

    private func load() {
        do {
            guard let decoded = try draftStore.load() else { return }
            for (key, value) in decoded {
                guard let scale = profileScales().first(where: { $0.key == key }),
                      value == 0 || scale.choices.contains(where: { $0.code == value }) else {
                    throw CocoaError(.fileReadCorruptFile)
                }
            }
            values = decoded
        } catch {
            readFailed = true
            status = "profile.readFailed"
        }
    }

    private func save() {
        do {
            try draftStore.save(values)
            status = "profile.saved"
        } catch {
            status = "profile.saveFailed"
        }
    }

}
