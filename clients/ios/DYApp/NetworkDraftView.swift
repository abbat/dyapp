import SwiftUI

/// Shared first connection form. Loading and saving the selected identity's draft follow separately.
struct NetworkDraftView: View {
    let candidate: IdentityCandidate
    @State private var bootstrapRows = ""
    @State private var networkName = ""

    var body: some View {
        Form {
            Section {
                TextEditor(text: $bootstrapRows)
                    .frame(minHeight: 120)
                    .accessibilityLabel(Text("network.bootstrap"))
                    .accessibilityIdentifier("network-bootstrap")
            } header: {
                Text("network.bootstrap")
            } footer: {
                Text("network.bootstrapHelp")
            }

            Section {
                TextField("network.name", text: $networkName)
                    .accessibilityIdentifier("network-name")
            } footer: {
                Text("network.nameHelp")
            }

            Section {
                Text("network.draftPending")
                    .accessibilityIdentifier("network-draft-status")
            }
        }
        .navigationTitle("network.title")
    }
}
