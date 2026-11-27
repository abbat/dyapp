import Foundation
import SwiftUI

/// One immutable sample request copied before native IO starts.
struct SearchRequest: @unchecked Sendable {
    let candidate: IdentityCandidate
    let peerId: String
    let conditions: [SearchCondition]
    let employer: SearchEmployer
    let mutual: Bool
}

struct SearchRun: Sendable {
    let owner: UUID
    let id: UUID
    let request: SearchRequest
}

private struct CachedSearchWorker: @unchecked Sendable {
    let value: ProfileSearcher?
}

struct SearchOutcome: @unchecked Sendable {
    let searcher: ProfileSearcher?
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
    private var searcher: ProfileSearcher?
    private var pending: UUID?
    private(set) var owner = UUID()

    /// Starts over when the selected identity changed since the screen was last open.
    func open(_ candidate: IdentityCandidate, peerId: String) {
        guard self.peerId != peerId || self.candidate?.location != candidate.location else {
            return
        }
        retire()
        self.candidate = candidate
        self.peerId = peerId
    }

    func context(for candidate: IdentityCandidate, peerId: String) -> UUID? {
        guard self.candidate?.location == candidate.location, self.peerId == peerId else {
            return nil
        }
        return owner
    }

    func begin(owner expected: UUID) -> SearchRun? {
        guard owner == expected, let candidate, !state.isLoading else { return nil }
        do {
            let run = SearchRun(
                owner: owner, id: UUID(),
                request: SearchRequest(
                    candidate: candidate, peerId: peerId,
                    conditions: try filters.conditions(), employer: filters.employer(),
                    mutual: filters.mutualWanted))
            pending = run.id
            state = .loading
            return run
        } catch {
            finish(.failure(error))
            return nil
        }
    }

    func run(owner expected: UUID) {
        guard let run = begin(owner: expected) else { return }
        let cached = CachedSearchWorker(value: searcher)
        Task {
            let outcome = await IdentityIO.performBlocking {
                let prepared = Result {
                    try cached.value
                        ?? RustBridge.profileSearcher(
                            for: run.request.candidate, peerId: run.request.peerId)
                }
                return SearchOutcome(
                    searcher: try? prepared.get(),
                    result: Result {
                        try RustBridge.searchProfiles(run.request, searcher: prepared.get())
                    })
            }
            complete(outcome, for: run)
        }
    }

    func complete(_ outcome: SearchOutcome, for run: SearchRun) {
        guard owner == run.owner, pending == run.id else { return }
        pending = nil
        searcher = outcome.searcher
        finish(outcome.result)
    }

    /// Abandons presentation only; the native operation retains its ownership through completion.
    func cancel(owner expected: UUID) {
        guard owner == expected else { return }
        pending = nil
        state = .idle
    }

    func retire() {
        owner = UUID()
        pending = nil
        candidate = nil
        peerId = ""
        searcher = nil
        filters = SearchFilterInputs()
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
            NSAccessibility.post(
                element: NSApp as Any, notification: .announcementRequested,
                userInfo: [
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
