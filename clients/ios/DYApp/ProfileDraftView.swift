import SwiftUI

/// A local draft editor. The shared Rust core supplies every numeric choice.
struct ProfileDraftView: View {
    let peerId: String
    @State private var values: [String: UInt32] = [:]
    @State private var status = ""
    @State private var readFailed = false

    private var draftURL: URL {
        get throws {
            try RustBridge.directory().appendingPathComponent("profile-draft-\(peerId).json")
        }
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
            let url = try draftURL
            guard FileManager.default.fileExists(atPath: url.path) else { return }
            let decoded = try JSONDecoder().decode([String: UInt32].self, from: Data(contentsOf: url))
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
            let data = try JSONEncoder().encode(values)
            try data.write(to: draftURL, options: .atomic)
            status = "profile.saved"
        } catch {
            status = "profile.saveFailed"
        }
    }

}
