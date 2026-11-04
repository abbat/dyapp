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
        var found = shownScales.compactMap { scale -> SearchCondition? in
            guard let code = picked[scale.key], code != 0 else { return nil }
            return SearchCondition(field: scale.field, op: .eq, ints: [Int64(code)], strings: [])
        }
        if ageOffered, !blank(ageMin) || !blank(ageMax) {
            found.append(try searchAgeCondition(min: try age(ageMin) ?? Self.youngest,
                                                max: try age(ageMax) ?? Self.oldest))
        }
        if heightOffered, let heights = try searchHeightCondition(from: try height(heightMin), to: try height(heightMax)) {
            found.append(heights)
        }
        if countryOffered, !country.isEmpty {
            found.append(SearchCondition(field: SearchField.country, op: .eq, ints: [], strings: [country]))
            if placeOffered, let place {
                found.append(SearchCondition(field: SearchField.place, op: .eq, ints: [], strings: [String(place)]))
            }
        }
        return found
    }

    func employer() -> SearchEmployer {
        let typed = company.trimmingCharacters(in: .whitespacesAndNewlines)
        switch (employerKind, typed.isEmpty) {
        case (.any, _), (_, true):
            return .any

        case (.same, false):
            return .same(company: typed)

        case (.exclude, false):
            return .exclude(companies: typed.split(separator: ";")
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .filter { !$0.isEmpty })
        }
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
let searchFixTheFields: Set<String> = ["search.ageInvalid", "search.heightInvalid", "search.mutualUnavailable"]

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
