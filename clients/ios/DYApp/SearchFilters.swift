import Foundation

private enum SearchField {
    static let country: UInt32 = 4
    static let place: UInt32 = 20
    static let height: UInt32 = 46
    static let seekingGenders: UInt32 = 53
    static let seekingAge: UInt32 = 54
    static let birthAge: UInt32 = 1_001
}

enum SearchEmployerKind: Hashable {
    case any, same, exclude

    var coreMode: SearchEmployerMode {
        switch self {
        case .any:
            .any

        case .same:
            .same

        case .exclude:
            .exclude
        }
    }
}

/// What the search form holds. The nodes name the fields they serve, so a filter shows only once a
/// sample has offered it, and the typed values stay when a later sample offers fewer.
struct SearchFilterInputs {
    private static let youngest: UInt32 = 18
    private static let oldest: UInt32 = 150

    var offered: [UInt32] = []
    var ageMin = ""
    var ageMax = ""
    var heightMin = ""
    var heightMax = ""
    var country = ""
    var place: UInt32?
    var placeText = ""
    var picked: [String: UInt32] = [:]
    var moreOpen = false
    var employerKind = SearchEmployerKind.any
    var company = ""
    var mutual = false

    var ageOffered: Bool { offered.contains(SearchField.birthAge) }
    var heightOffered: Bool { offered.contains(SearchField.height) }
    var countryOffered: Bool { offered.contains(SearchField.country) }
    var placeOffered: Bool { offered.contains(SearchField.place) }
    var mutualOffered: Bool {
        offered.contains(SearchField.seekingGenders) && offered.contains(SearchField.seekingAge)
    }
    var shownScales: [ProfileScale] { profileScales().filter { offered.contains($0.field) } }
    var mutualWanted: Bool { mutual && mutualOffered }

    /// The shared core refuses a reversed age or height range with the same errors as a bad number.
    func conditions() throws -> [SearchCondition] {
        var found = searchChoiceConditions(
            offered: offered,
            inputs: SearchChoiceInputs(
                picked: picked.mapValues { Int64($0) }, country: country, interest: "", place: place
            ))
        if ageOffered, !blank(ageMin) || !blank(ageMax) {
            found.append(
                try searchAgeCondition(
                    min: try age(ageMin) ?? Self.youngest,
                    max: try age(ageMax) ?? Self.oldest))
        }
        if heightOffered,
            let heights = try searchHeightCondition(
                from: try height(heightMin), to: try height(heightMax))
        {
            found.append(heights)
        }
        return found
    }

    func employer() -> SearchEmployer {
        searchEmployerFilter(mode: employerKind.coreMode, text: company)
    }

    private func blank(_ text: String) -> Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func age(_ typed: String) throws -> UInt32? {
        if blank(typed) { return nil }
        guard let value = UInt32(typed.trimmingCharacters(in: .whitespacesAndNewlines)) else {
            throw CoreError.Failed(message: "invalid search age")
        }
        return value
    }

    private func height(_ typed: String) throws -> UInt32? {
        if blank(typed) { return nil }
        guard let value = UInt32(typed.trimmingCharacters(in: .whitespacesAndNewlines)) else {
            throw CoreError.Failed(message: "invalid search height")
        }
        return value
    }
}

/// Status texts that need the fields fixed, not a connection or the same query again.
let searchFixTheFields: Set<String> = [
    "search.ageInvalid", "search.heightInvalid", "search.mutualUnavailable",
]

/// The status text for a failed sample; both the form and the shared core use these error messages.
func searchErrorKey(_ error: Error) -> String {
    let text = String(describing: error)
    let known = [
        ("notConnected", "search.notConnected"),
        ("invalid search age", "search.ageInvalid"),
        ("invalid search height", "search.heightInvalid"),
        ("mutual unavailable", "search.mutualUnavailable"),
    ]
    return known.first { text.contains($0.0) }?.1 ?? "search.failed"
}
