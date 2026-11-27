private enum SearchField {
    static let country: UInt32 = 4
    static let interest: UInt32 = 19
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
    var offered: [UInt32] = []
    var ageMin = ""
    var ageMax = ""
    var heightMin = ""
    var heightMax = ""
    var country = ""
    var interest = ""
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
    var interestOffered: Bool { offered.contains(SearchField.interest) }
    var placeOffered: Bool { offered.contains(SearchField.place) }
    var mutualOffered: Bool {
        offered.contains(SearchField.seekingGenders) && offered.contains(SearchField.seekingAge)
    }
    var shownScales: [ProfileScale] { profileScales().filter { offered.contains($0.field) } }
    var mutualWanted: Bool { mutual && mutualOffered }

    /// The shared core refuses a reversed age or height range with the same errors as a bad number.
    func conditions() throws -> [SearchCondition] {
        let found = try searchCheckedChoiceConditions(
            offered: offered,
            inputs: SearchChoiceInputs(
                picked: picked.mapValues { Int64($0) }, country: country, interest: interest,
                place: place
            ))
        let ranges = try searchRangeConditions(
            offered: offered,
            inputs: SearchRangeInputs(
                ageMin: ageMin, ageMax: ageMax, heightMin: heightMin, heightMax: heightMax
            ))
        return found + ranges
    }

    func employer() -> SearchEmployer {
        searchEmployerFilter(mode: employerKind.coreMode, text: company)
    }

}

/// Status texts that need the fields fixed, not a connection or the same query again.
let searchFixTheFields: Set<String> = [
    "search.ageInvalid", "search.heightInvalid", "search.mutualUnavailable",
    "search.interestInvalid",
]

/// The status text for a failed sample; both the form and the shared core use these error messages.
func searchErrorKey(_ error: Error) -> String {
    let text = String(describing: error)
    let known = [
        ("notConnected", "search.notConnected"),
        ("invalid search age", "search.ageInvalid"),
        ("invalid search height", "search.heightInvalid"),
        ("invalid search interest", "search.interestInvalid"),
        ("mutual unavailable", "search.mutualUnavailable"),
    ]
    return known.first { text.contains($0.0) }?.1 ?? "search.failed"
}
