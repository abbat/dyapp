import Foundation
import SwiftUI

/// One blocking sample; the UniFFI values stay inside the detached task and back on the main actor.
struct SearchRequest: @unchecked Sendable {
    let searcher: ProfileSearcher
    let candidate: IdentityCandidate
    let peerId: String
    let conditions: [SearchCondition]
    let employer: SearchEmployer
    let mutual: Bool
}

private struct SearchOutcome: @unchecked Sendable {
    let result: Result<ProfileSearchResult, Error>
}

enum SearchState {
    case idle, loading
    case done(ProfileSearchResult)
    case failed(key: String, details: String)

    var isLoading: Bool {
        if case .loading = self { return true }
        return false
    }
}

/// One identity's search. ContentView owns it, so filters and results outlive the pages opened over them.
@MainActor
final class SearchSession: ObservableObject {
    @Published var filters = SearchFilterInputs()
    @Published private(set) var state = SearchState.idle
    private var candidate: IdentityCandidate?
    private var peerId = ""
    // The searcher keeps verified versions, so a later sample never shows an older one.
    private var searcher: Result<ProfileSearcher, Error>?
    private var generation = 0

    /// Starts over when the selected identity changed since the screen was last open.
    func open(_ candidate: IdentityCandidate, peerId: String) {
        guard self.peerId != peerId || searcher == nil else { return }
        generation += 1
        self.candidate = candidate
        self.peerId = peerId
        filters = SearchFilterInputs()
        state = .idle
        searcher = Result { try RustBridge.profileSearcher(for: candidate) }
    }

    func run() {
        guard let candidate, let searcher else { return }
        let request: SearchRequest
        do {
            request = SearchRequest(searcher: try searcher.get(), candidate: candidate, peerId: peerId,
                                    conditions: try filters.conditions(), employer: filters.employer(),
                                    mutual: filters.mutualWanted)
        } catch {
            finish(.failure(error))
            return
        }
        generation += 1
        let current = generation
        state = .loading
        Task {
            let outcome = await Task.detached(priority: .userInitiated) {
                SearchOutcome(result: Result { try RustBridge.searchProfiles(request) })
            }.value
            guard current == generation else { return }
            finish(outcome.result)
        }
    }

    /// Abandons the answer; the blocking call itself runs to its end.
    func cancel() {
        generation += 1
        state = .idle
    }

    private func finish(_ result: Result<ProfileSearchResult, Error>) {
        switch result {
        case let .success(found):
            filters.offered = found.offered
            state = .done(found)
            announce(searchCountText(found))

        case let .failure(error):
            let key = searchErrorKey(error)
            state = .failed(key: key, details: String(describing: error))
            announce(NSLocalizedString(key, comment: ""))
        }
    }

    private func announce(_ text: String) {
        #if os(iOS)
        UIAccessibility.post(notification: .announcement, argument: text)
        #else
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested, userInfo: [
            .announcement: text,
            .priority: NSAccessibilityPriorityLevel.high.rawValue,
        ])
        #endif
    }
}

/// The heading of a finished sample: how many verified profiles it holds, or that it holds none.
func searchCountText(_ result: ProfileSearchResult) -> String {
    if result.profiles.isEmpty { return NSLocalizedString("search.empty", comment: "") }
    let key = result.partial ? "search.partial" : "search.results"
    return String(format: NSLocalizedString(key, comment: ""), result.profiles.count)
}
