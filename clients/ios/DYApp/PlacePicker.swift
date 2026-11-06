import SwiftUI

/// "Berlin" or "Berlin, Brandenburg".
func placeLabel(_ place: Place) -> String {
    place.region.isEmpty ? place.name : "\(place.name), \(place.region)"
}

/// The UI language for Rust place names: `ru` gives Russian, any other English.
func placeLang() -> String { Bundle.main.preferredLocalizations.first ?? "en" }

func countryName(_ code: String) -> String { Locale.current.localizedString(forRegionCode: code) ?? code }

/// Country and place rows of the profile editor. `place` is set only by picking a suggestion;
/// typed text clears it, and a new country clears both.
struct PlacePicker: View {
    @Binding var country: String
    @Binding var place: UInt32?
    @Binding var text: String
    let disabled: Bool
    /// Search reuses these rows with its own ids, its "any" wording and only the fields a node offers.
    var idPrefix = "profile"
    var anyLabel: LocalizedStringKey = "profile.unset"
    var placeShown = true
    #if os(macOS)
    @State private var choosing = false
    #endif

    var body: some View {
        countryRow
            .disabled(disabled)
            .accessibilityIdentifier(idPrefix + "-country")
        if placeShown { placeRows }
    }

    @ViewBuilder private var countryRow: some View {
        #if os(macOS)
        // A push from the macOS 12 detail column has no way back: the list opens as a sheet.
        Button { choosing = true } label: { countryLabel }
            .sheet(isPresented: $choosing) { CountryList(anyLabel: anyLabel, pick: pickCountry) }
        #else
        NavigationLink { CountryList(anyLabel: anyLabel, pick: pickCountry) } label: { countryLabel }
        #endif
    }

    private var countryLabel: some View {
        HStack {
            Text("profile.country")
            Spacer()
            if country.isEmpty { Text(anyLabel) } else { Text(verbatim: countryName(country)) }
        }
    }

    @ViewBuilder private var placeRows: some View {
        TextField("profile.place", text: Binding(get: { text }, set: { typed in
            guard typed != text else { return }
            text = typed
            place = nil
        }))
        .disabled(disabled || country.isEmpty)
        .accessibilityIdentifier(idPrefix + "-place")
        if place == nil && !text.isEmpty {
            ForEach(searchPlaces(country: country, prefix: text, lang: placeLang(), limit: 5), id: \.id) { found in
                Button(placeLabel(found)) {
                    place = found.id
                    text = placeLabel(found)
                }
                .disabled(disabled)
                .accessibilityIdentifier("place-\(found.id)")
            }
        }
    }

    private func pickCountry(_ code: String) {
        guard code != country else { return }
        country = code
        place = nil
        text = ""
    }
}

/// Every picker country by its localized name, filtered by the search text.
private struct CountryList: View {
    let anyLabel: LocalizedStringKey
    let pick: (String) -> Void
    @State private var query = ""
    @Environment(\.dismiss) private var dismiss
    private let countries = placeCountries().map { (code: $0, name: countryName($0)) }
        .sorted { $0.name.localizedCompare($1.name) == .orderedAscending }

    var body: some View {
        #if os(macOS)
        // A sheet has no toolbar for `.searchable`: a plain field and Cancel, reusing the search wording.
        VStack {
            TextField("search.title", text: $query).accessibilityIdentifier("country-search")
            list
            Button("search.cancel") { dismiss() }.accessibilityIdentifier("country-cancel")
        }
        .padding()
        .frame(minWidth: 320, minHeight: 420)
        #else
        list
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always))
            .navigationTitle("profile.country")
        #endif
    }

    private var list: some View {
        List {
            CountryRows(anyLabel: anyLabel, countries: countries.filter {
                query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
            }, choose: choose)
        }
    }

    private func choose(_ code: String) {
        pick(code)
        dismiss()
    }
}

/// The country rows, inside the search so they can end it: a pop with the search field still
/// active was followed in UI tests by an app that never again reported its animations complete.
private struct CountryRows: View {
    let anyLabel: LocalizedStringKey
    let countries: [(code: String, name: String)]
    let choose: (String) -> Void
    @Environment(\.dismissSearch) private var dismissSearch

    var body: some View {
        Button(anyLabel) { pick("") }
            .accessibilityIdentifier("country-none")
        ForEach(countries, id: \.code) { country in
            Button(country.name) { pick(country.code) }
                .accessibilityIdentifier("country-\(country.code)")
        }
    }

    private func pick(_ code: String) {
        dismissSearch()
        choose(code)
    }
}
