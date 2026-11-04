import SwiftUI

/// Who the owner is looking for; a saved `notLooking` comes without the other three.
struct ProfileSeeking: Equatable, Sendable {
    var genders: [UInt32] = []
    var ageFrom: UInt32?
    var ageTo: UInt32?
    var notLooking = false
}

/// A number that is no whole number; its raw value is the status text.
enum ProfileInputError: String, Error {
    case height = "profile.invalidHeight"
    case seeking = "profile.invalidSeeking"
}

/// The status text for a refused save; the shared core names the field it refused.
func profileSaveStatus(_ error: Error) -> String {
    guard case CoreError.Failed(let refusal) = error else { return "profile.saveFailed" }
    return [
        "invalidName": "profile.invalidName",
        "invalidHeight": ProfileInputError.height.rawValue,
        "invalidAbout": "profile.invalidAbout",
        "invalidSeeking": ProfileInputError.seeking.rawValue,
    ][refusal] ?? "profile.saveFailed"
}

/// The typed fields beyond the scales; the shared core checks ranges and gender codes on save.
struct ProfileExtraInputs: Equatable {
    var name = ""
    var height = ""
    var about = ""
    var genders: Set<UInt32> = []
    var ageFrom = ""
    var ageTo = ""
    var notLooking = false

    /// Compared for unsaved edits; the switch makes the choices it disables irrelevant.
    var tracked: Self {
        guard notLooking else { return self }
        var kept = self
        kept.genders = []
        kept.ageFrom = ""
        kept.ageTo = ""
        return kept
    }

    init() {}

    init(draft: ProfileDraft) {
        name = draft.name
        height = draft.height.map(String.init) ?? ""
        about = draft.about
        genders = Set(draft.seeking.genders)
        ageFrom = draft.seeking.ageFrom.map(String.init) ?? ""
        ageTo = draft.seeking.ageTo.map(String.init) ?? ""
        notLooking = draft.seeking.notLooking
    }

    /// `draft` with these fields as the store keeps them.
    func inserted(into draft: ProfileDraft) throws -> ProfileDraft {
        func number(_ text: String, _ refusal: ProfileInputError) throws -> UInt32? {
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty { return nil }
            guard let value = UInt32(trimmed) else { throw refusal }
            return value
        }
        var draft = draft
        draft.name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        draft.height = try number(height, .height)
        draft.about = about.trimmingCharacters(in: .whitespacesAndNewlines)
        draft.seeking = notLooking ? ProfileSeeking(notLooking: true) : ProfileSeeking(
            genders: genders.sorted(),
            ageFrom: try number(ageFrom, .seeking),
            ageTo: try number(ageTo, .seeking)
        )
        return draft
    }
}

private func genderChoices() -> [ProfileChoice] {
    profileScales().first { $0.key == "gender" }?.choices ?? []
}

/// Public name, height and about, then the seeking choices the switch disables; they stay visible
/// so that turning it off restores them before a save.
struct ProfileExtraFields: View {
    @Binding var inputs: ProfileExtraInputs
    let disabled: Bool

    var body: some View {
        Group {
            TextField("profile.name", text: $inputs.name)
                .accessibilityIdentifier("profile-name")
            Text("profile.nameHint").font(.caption)
            TextField("profile.height", text: $inputs.height)
                .accessibilityIdentifier("profile-height")
            Section(header: Text("profile.about")) {
                TextEditor(text: $inputs.about)
                    .frame(minHeight: 80)
                    .accessibilityLabel(Text("profile.about"))
                    .accessibilityIdentifier("profile-about")
                Text("profile.aboutHint").font(.caption)
            }
            Section(header: Text("profile.seekingTitle")) {
                Toggle("profile.notLooking", isOn: $inputs.notLooking)
                    .accessibilityIdentifier("profile-not-looking")
                Text("profile.notLookingHint").font(.caption)
                Text("profile.seekingGenders")
                ForEach(genderChoices(), id: \.code) { choice in
                    Toggle(LocalizedStringKey("profile.value.gender." + choice.key), isOn: Binding(
                        get: { inputs.genders.contains(choice.code) },
                        set: { if $0 { inputs.genders.insert(choice.code) } else { inputs.genders.remove(choice.code) } }
                    ))
                    .disabled(inputs.notLooking)
                    .accessibilityIdentifier("profile-seeking-gender-\(choice.code)")
                }
                TextField("profile.seekingAgeFrom", text: $inputs.ageFrom)
                    .disabled(inputs.notLooking)
                    .accessibilityIdentifier("profile-seeking-age-from")
                TextField("profile.seekingAgeTo", text: $inputs.ageTo)
                    .disabled(inputs.notLooking)
                    .accessibilityIdentifier("profile-seeking-age-to")
                Text("profile.seekingHint").font(.caption)
            }
        }
        .disabled(disabled)
    }
}

/// The public rows of a saved draft's typed fields; a gender code this client does not know is left out.
struct ProfileExtraRows: View {
    let draft: ProfileDraft

    private var genders: String {
        genderChoices().filter { draft.seeking.genders.contains($0.code) }
            .map { NSLocalizedString("profile.value.gender." + $0.key, comment: "") }
            .joined(separator: ", ")
    }

    var body: some View {
        if !draft.name.isEmpty { row("profile.nameRow", Text(verbatim: draft.name), "preview-name") }
        if let height = draft.height { row("profile.height", Text(verbatim: String(height)), "preview-height") }
        if !draft.about.isEmpty { row("profile.about", Text(verbatim: draft.about), "preview-about") }
        if draft.seeking.notLooking { row("profile.notLooking", Text("profile.notLookingOn"), "preview-not-looking") }
        if !genders.isEmpty { row("profile.seekingGenders", Text(verbatim: genders), "preview-seeking-genders") }
        if let from = draft.seeking.ageFrom {
            row("profile.seekingAgeFrom", Text(verbatim: String(from)), "preview-seeking-age-from")
        }
        if let to = draft.seeking.ageTo {
            row("profile.seekingAgeTo", Text(verbatim: String(to)), "preview-seeking-age-to")
        }
    }

    private func row(_ key: String, _ value: Text, _ id: String) -> some View {
        HStack {
            Text(LocalizedStringKey(key))
            Spacer()
            value
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(id)
    }
}
