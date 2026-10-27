import Foundation

struct ProfileWorkProgress: Sendable {
    let solved: Bool
    let cancelled: Bool
    let nextNonce: UInt64
}

enum ProfileWorkAttempt: Sendable {
    case progress(ProfileWorkProgress)
    case failed
}

/// UniFFI's Rust object synchronizes each step and accepts cancellation on another thread.
final class ProfileSolveSession: @unchecked Sendable {
    private let solver: ProfileSolve

    init(solver: ProfileSolve) {
        self.solver = solver
    }

    func runStep(attested: Bool) async -> ProfileWorkAttempt {
        await Task.detached(priority: .userInitiated) { [self] () -> ProfileWorkAttempt in
            do {
                let step = try solver.runStep(attested: attested)
                return .progress(ProfileWorkProgress(solved: step.solved, cancelled: step.cancelled,
                                                     nextNonce: step.nextNonce))
            } catch {
                return .failed
            }
        }.value
    }

    func cancel() {
        solver.cancel()
    }
}
